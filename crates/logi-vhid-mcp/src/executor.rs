use crate::params::{
    BackendKind, CleanupResult, ErrorCode, InputRequest, OperationOutcome, OperationState,
    ReleaseState, Submission, ToolFailure, ToolResponse, WorkerState,
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc, Mutex, MutexGuard,
};
use std::time::{Duration, Instant};
use tokio::sync::{oneshot, Notify};
use tokio_util::sync::CancellationToken;

pub(crate) trait InputBackend {
    fn execute(
        &mut self,
        request: InputRequest,
        control: &OperationControl,
    ) -> ToolResponse<OperationOutcome>;
    fn release(&mut self) -> CleanupResult;
}

#[derive(Clone)]
pub(crate) struct OperationControl {
    pub(crate) request_ct: CancellationToken,
    pub(crate) stop_ct: CancellationToken,
    pub(crate) shutdown_ct: CancellationToken,
    pub(crate) deadline: Instant,
}

impl OperationControl {
    pub(crate) fn check(&self) -> logi_vhid::Result<()> {
        if self.request_ct.is_cancelled()
            || self.stop_ct.is_cancelled()
            || self.shutdown_ct.is_cancelled()
        {
            Err(logi_vhid::Error::Cancelled)
        } else if Instant::now() >= self.deadline {
            Err(logi_vhid::Error::TimedOut)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone)]
pub(crate) struct Executor {
    inner: Arc<Inner>,
}

pub(crate) struct ExecutorSnapshot {
    pub(crate) worker_state: WorkerState,
    pub(crate) device_initialized: bool,
    pub(crate) last_operation: Option<ToolResponse<OperationOutcome>>,
}

struct Inner {
    shared: Arc<Shared>,
    commands: mpsc::Sender<Command>,
    allow_input: bool,
}

struct Shared {
    state: Mutex<State>,
    changed: Notify,
    initialized: Arc<AtomicBool>,
    shutdown_ct: CancellationToken,
}

struct State {
    worker_state: WorkerState,
    active: Option<Active>,
    stop: Option<Arc<StopTicket>>,
    last_operation: Option<ToolResponse<OperationOutcome>>,
    shutdown_started: bool,
    shutdown_result: Option<Result<(), String>>,
}

struct Active {
    stop_ct: CancellationToken,
    backends: Vec<BackendKind>,
}

struct StopTicket {
    result: Mutex<Option<ToolResponse<OperationOutcome>>>,
    changed: Notify,
}

enum Command {
    Execute {
        request: InputRequest,
        control: OperationControl,
        reply: oneshot::Sender<ToolResponse<OperationOutcome>>,
    },
    Release,
    Shutdown,
}

struct CancelOnDrop(CancellationToken);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        // Dropping all handlers must not strand an operation. Explicit shutdown
        // additionally awaits completion; Drop itself cannot block an async task.
        self.shared.shutdown_ct.cancel();
        let _ = self.commands.send(Command::Shutdown);
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl Executor {
    pub(crate) fn new(allow_input: bool) -> Self {
        Self::with_backend(allow_input, crate::native::NativeBackend::new)
    }

    pub(crate) fn with_backend<F, B>(allow_input: bool, factory: F) -> Self
    where
        F: FnOnce(Arc<AtomicBool>) -> B + Send + 'static,
        B: InputBackend + 'static,
    {
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                worker_state: WorkerState::Idle,
                active: None,
                stop: None,
                last_operation: None,
                shutdown_started: false,
                shutdown_result: None,
            }),
            changed: Notify::new(),
            initialized: Arc::new(AtomicBool::new(false)),
            shutdown_ct: CancellationToken::new(),
        });
        let (commands, receiver) = mpsc::channel();
        let worker_shared = shared.clone();
        let spawn = std::thread::Builder::new()
            .name("logi-vhid-input".into())
            .spawn(move || {
                // The backend is constructed, used and dropped here. In
                // particular B has no Send bound: native handles never move.
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    worker_main(factory, receiver, &worker_shared)
                }))
                .unwrap_or_else(|_| Err("input worker panicked; release is unconfirmed".into()));
                worker_exited(&worker_shared, result);
            });
        if let Err(error) = spawn {
            worker_exited(
                &shared,
                Err(format!("could not start input worker: {error}")),
            );
        }
        Self {
            inner: Arc::new(Inner {
                shared,
                commands,
                allow_input,
            }),
        }
    }

    pub(crate) fn snapshot(&self) -> ExecutorSnapshot {
        let state = lock(&self.inner.shared.state);
        ExecutorSnapshot {
            worker_state: state.worker_state,
            device_initialized: self.inner.shared.initialized.load(Ordering::Acquire),
            last_operation: state.last_operation.clone(),
        }
    }

    pub(crate) async fn submit(
        &self,
        request: InputRequest,
        request_ct: CancellationToken,
    ) -> ToolResponse<OperationOutcome> {
        if let Err(error) = request.validate() {
            return ToolResponse::failure(error);
        }
        let backends = request.selected_backends();
        // This precedes every backend/native call, even target validation.
        if !self.inner.allow_input {
            let response = rejected(backends, ErrorCode::InputDisabled, "input is disabled");
            lock(&self.inner.shared.state).last_operation = Some(response.clone());
            self.inner.shared.changed.notify_waiters();
            return response;
        }
        let request_ct = request_ct.child_token();
        let cancel_on_drop = CancelOnDrop(request_ct.clone());
        let (reply, receiver) = oneshot::channel();
        let shared = &self.inner.shared;
        {
            let mut state = lock(&shared.state);
            let refusal = match state.worker_state {
                WorkerState::Closed => Some((ErrorCode::WorkerClosed, "input worker is closed")),
                WorkerState::Faulted => Some((
                    ErrorCode::CleanupFailed,
                    "input cleanup failed; stop must release inputs before another operation",
                )),
                WorkerState::Running | WorkerState::Stopping => Some((
                    ErrorCode::Busy,
                    "an input operation or stop is already in progress",
                )),
                WorkerState::Idle if state.shutdown_started => {
                    Some((ErrorCode::WorkerClosed, "input worker is shutting down"))
                }
                WorkerState::Idle => None,
            };
            if let Some((code, message)) = refusal {
                let response = rejected(backends, code, message);
                if code != ErrorCode::WorkerClosed {
                    state.last_operation = Some(response.clone());
                }
                drop(state);
                shared.changed.notify_waiters();
                return response;
            }
            let stop_ct = CancellationToken::new();
            let control = OperationControl {
                request_ct,
                stop_ct: stop_ct.clone(),
                shutdown_ct: shared.shutdown_ct.clone(),
                deadline: Instant::now() + Duration::from_millis(request.timeout_ms()),
            };
            state.active = Some(Active { stop_ct, backends });
            state.worker_state = WorkerState::Running;
            // This channel never buffers a second input: admission and sending
            // are one short critical section, with no device work under lock.
            if self
                .inner
                .commands
                .send(Command::Execute {
                    request,
                    control,
                    reply,
                })
                .is_err()
            {
                // The worker may still be unwinding native destructors. Only
                // its exit path may acknowledge completed thread teardown.
                state.worker_state = WorkerState::Stopping;
            }
        }
        shared.changed.notify_waiters();
        let response = match receiver.await {
            Ok(response) => response,
            Err(_) => loop {
                let notified = shared.changed.notified();
                tokio::pin!(notified);
                notified.as_mut().enable();
                {
                    let state = lock(&shared.state);
                    if state.shutdown_result.is_some() {
                        break state.last_operation.clone().unwrap_or_else(|| {
                            rejected(
                                Vec::new(),
                                ErrorCode::WorkerClosed,
                                "input worker is closed",
                            )
                        });
                    }
                }
                notified.await;
            },
        };
        drop(cancel_on_drop);
        response
    }

    pub(crate) async fn stop(&self) -> ToolResponse<OperationOutcome> {
        let shared = &self.inner.shared;
        let ticket;
        {
            let mut state = lock(&shared.state);
            if state.worker_state == WorkerState::Closed || state.shutdown_started {
                return rejected(
                    Vec::new(),
                    ErrorCode::WorkerClosed,
                    "input worker is closed or shutting down",
                );
            }
            if let Some(existing) = &state.stop {
                ticket = existing.clone();
            } else if state.active.is_none() && !shared.initialized.load(Ordering::Acquire) {
                let response = ToolResponse::success(stop_outcome(
                    OperationState::Completed,
                    CleanupResult::not_opened(),
                ));
                state.worker_state = WorkerState::Idle;
                state.last_operation = Some(response.clone());
                drop(state);
                shared.changed.notify_waiters();
                return response;
            } else {
                ticket = Arc::new(StopTicket {
                    result: Mutex::new(None),
                    changed: Notify::new(),
                });
                state.stop = Some(ticket.clone());
                state.worker_state = WorkerState::Stopping;
                if let Some(active) = &state.active {
                    active.stop_ct.cancel();
                } else {
                    // If disconnected, worker_exited will settle this ticket
                    // after the backend has actually been dropped.
                    let _ = self.inner.commands.send(Command::Release);
                }
            }
        }
        shared.changed.notify_waiters();
        match tokio::time::timeout(Duration::from_secs(2), ticket.wait()).await {
            Ok(response) => response,
            Err(_) => {
                // Check once more after the timeout, so completed cleanup is
                // never replaced by a stale pending result.
                if let Some(response) = lock(&ticket.result).clone() {
                    return response;
                }
                ToolResponse::success(stop_outcome(
                    OperationState::Stopping,
                    // An in-flight open can have acquired handles before its
                    // initialized flag is published. Unknown is not unopened.
                    CleanupResult::pending(true),
                ))
            }
        }
    }

    pub(crate) async fn shutdown(&self) -> Result<(), String> {
        let shared = &self.inner.shared;
        shared.shutdown_ct.cancel();
        {
            let mut state = lock(&shared.state);
            if !state.shutdown_started && state.shutdown_result.is_none() {
                state.shutdown_started = true;
                state.worker_state = WorkerState::Stopping;
                if let Some(active) = &state.active {
                    active.stop_ct.cancel();
                }
                // A disconnected receiver is settled by the worker exit path,
                // not by this async thread before native destructors finish.
                let _ = self.inner.commands.send(Command::Shutdown);
            }
        }
        shared.changed.notify_waiters();
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let notified = shared.changed.notified();
                tokio::pin!(notified);
                notified.as_mut().enable();
                if let Some(result) = lock(&shared.state).shutdown_result.clone() {
                    return result;
                }
                notified.await;
            }
        })
        .await
        .unwrap_or_else(|_| Err("input worker shutdown timed out; release is unconfirmed".into()))
    }
}

impl StopTicket {
    async fn wait(&self) -> ToolResponse<OperationOutcome> {
        loop {
            let notified = self.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if let Some(response) = lock(&self.result).clone() {
                return response;
            }
            notified.await;
        }
    }

    fn complete(&self, response: ToolResponse<OperationOutcome>) {
        *lock(&self.result) = Some(response);
        self.changed.notify_waiters();
    }
}

fn rejected(
    backends: Vec<BackendKind>,
    code: ErrorCode,
    message: &str,
) -> ToolResponse<OperationOutcome> {
    ToolResponse::with_error(
        OperationOutcome::rejected(backends),
        ToolFailure::new(code, message),
    )
}

fn cleanup_unconfirmed(cleanup: &CleanupResult) -> bool {
    // Earlier sequence-cleanup errors remain useful evidence, but only final
    // category states decide whether another operation can be accepted.
    matches!(
        cleanup.keyboard,
        ReleaseState::Failed | ReleaseState::Pending
    ) || matches!(cleanup.mouse, ReleaseState::Failed | ReleaseState::Pending)
}

fn stop_outcome(state: OperationState, cleanup: CleanupResult) -> OperationOutcome {
    OperationOutcome {
        state,
        submission: Submission::None,
        selected_backends: Vec::new(),
        cleanup,
        cursor_after: None,
        target_observed: false,
    }
}

fn release_response(cleanup: CleanupResult) -> ToolResponse<OperationOutcome> {
    if cleanup_unconfirmed(&cleanup) {
        let mut error = ToolFailure::new(ErrorCode::CleanupFailed, "input release failed");
        error.native = cleanup.errors.first().cloned();
        ToolResponse::with_error(stop_outcome(OperationState::Failed, cleanup), error)
    } else {
        ToolResponse::success(stop_outcome(OperationState::Completed, cleanup))
    }
}

fn worker_main<F, B>(
    factory: F,
    commands: mpsc::Receiver<Command>,
    shared: &Shared,
) -> Result<(), String>
where
    F: FnOnce(Arc<AtomicBool>) -> B,
    B: InputBackend,
{
    let mut backend = factory(shared.initialized.clone());
    while let Ok(command) = commands.recv() {
        match command {
            Command::Execute {
                request,
                control,
                reply,
            } => {
                let mut response = backend.execute(request, &control);
                let faulted = response
                    .data
                    .as_ref()
                    .map(|outcome| cleanup_unconfirmed(&outcome.cleanup))
                    .unwrap_or(true);
                if faulted {
                    response.ok = false;
                    if let Some(error) = &mut response.error {
                        error.code = ErrorCode::CleanupFailed;
                    } else {
                        let mut error = ToolFailure::new(
                            ErrorCode::CleanupFailed,
                            "input cleanup failed; release is unconfirmed",
                        );
                        error.native = response
                            .data
                            .as_ref()
                            .and_then(|outcome| outcome.cleanup.errors.first().cloned());
                        response.error = Some(error);
                    }
                    if let Some(outcome) = &mut response.data {
                        outcome.state = OperationState::Failed;
                    }
                }
                // execute owns the finally-release. Never duplicate it here.
                let stop = {
                    let mut state = lock(&shared.state);
                    state.active = None;
                    state.worker_state = if state.shutdown_started {
                        WorkerState::Stopping
                    } else if faulted {
                        WorkerState::Faulted
                    } else {
                        WorkerState::Idle
                    };
                    if let Some(stop) = state.stop.take() {
                        let stop_response = response
                            .data
                            .as_ref()
                            .map(|outcome| release_response(outcome.cleanup.clone()))
                            .unwrap_or_else(|| {
                                ToolResponse::with_error(
                                    stop_outcome(
                                        OperationState::Failed,
                                        CleanupResult::pending(true),
                                    ),
                                    ToolFailure::new(
                                        ErrorCode::CleanupFailed,
                                        "backend returned no cleanup result",
                                    ),
                                )
                            });
                        state.last_operation = Some(stop_response.clone());
                        Some((stop, stop_response))
                    } else {
                        state.last_operation = Some(response.clone());
                        None
                    }
                };
                if let Some((stop, stop_response)) = stop {
                    stop.complete(stop_response);
                }
                shared.changed.notify_waiters();
                let _ = reply.send(response);
            }
            Command::Release => {
                let response = release_response(backend.release());
                let stop = {
                    let mut state = lock(&shared.state);
                    state.worker_state = if state.shutdown_started {
                        WorkerState::Stopping
                    } else if response.ok {
                        WorkerState::Idle
                    } else {
                        WorkerState::Faulted
                    };
                    state.last_operation = Some(response.clone());
                    state.stop.take()
                };
                if let Some(stop) = stop {
                    stop.complete(response);
                }
                shared.changed.notify_waiters();
            }
            Command::Shutdown => {
                let cleanup = backend.release();
                // Explicit drop precedes the closed notification: native mutex
                // ownership ends on this thread, before shutdown acknowledges.
                drop(backend);
                return if cleanup_unconfirmed(&cleanup) {
                    Err("input worker shutdown cleanup failed".into())
                } else {
                    Ok(())
                };
            }
        }
    }
    let cleanup = backend.release();
    drop(backend);
    if cleanup_unconfirmed(&cleanup) {
        Err("input worker channel closed and cleanup failed".into())
    } else {
        Ok(())
    }
}

fn worker_exited(shared: &Shared, result: Result<(), String>) {
    let stop;
    let response;
    {
        let mut state = lock(&shared.state);
        if state.shutdown_result.is_some() {
            return;
        }
        let had_active = state.active.is_some();
        let had_stop = state.stop.is_some();
        response = if let Some(active) = state.active.take() {
            let mut outcome = OperationOutcome::rejected(active.backends);
            outcome.state = OperationState::Failed;
            outcome.submission = Submission::PartialOrUnknown;
            outcome.cleanup = CleanupResult::pending(true);
            ToolResponse::with_error(
                outcome,
                ToolFailure::new(
                    ErrorCode::WorkerClosed,
                    "input worker ended; release is unconfirmed",
                ),
            )
        } else if had_stop {
            ToolResponse::with_error(
                stop_outcome(OperationState::Failed, CleanupResult::pending(true)),
                ToolFailure::new(ErrorCode::WorkerClosed, "input worker ended during release"),
            )
        } else {
            rejected(
                Vec::new(),
                ErrorCode::WorkerClosed,
                "input worker is closed",
            )
        };
        if had_active || had_stop {
            state.last_operation = Some(response.clone());
        }
        state.worker_state = WorkerState::Closed;
        state.shutdown_result = Some(result);
        stop = state.stop.take();
    }
    if let Some(stop) = stop {
        stop.complete(response);
    }
    shared.changed.notify_waiters();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::{KeyParams, ReleaseState};
    use std::rc::Rc;
    use std::sync::atomic::AtomicUsize;
    use tokio::sync::oneshot;

    fn request() -> InputRequest {
        InputRequest::Key(
            serde_json::from_value::<KeyParams>(serde_json::json!({
                "target": {"hwnd": "0x1", "pid": 1}, "keys": ["A"]
            }))
            .unwrap(),
        )
    }

    struct Backend {
        initialized: Arc<AtomicBool>,
        actions: Arc<AtomicUsize>,
        releases: Arc<AtomicUsize>,
        failures: Arc<AtomicUsize>,
        entered: Option<oneshot::Sender<()>>,
        wait_for_cancel: bool,
        release_gate: Option<std::sync::mpsc::Receiver<()>>,
        panic: bool,
        // Models a native thread-affine backend: it deliberately is not Send.
        _thread_affine: Rc<()>,
    }

    impl InputBackend for Backend {
        fn execute(
            &mut self,
            request: InputRequest,
            control: &OperationControl,
        ) -> ToolResponse<OperationOutcome> {
            if self.panic {
                panic!("simulated backend failure");
            }
            self.initialized.store(true, Ordering::Release);
            self.actions.fetch_add(1, Ordering::SeqCst);
            if let Some(entered) = self.entered.take() {
                let _ = entered.send(());
            }
            let cause = if self.wait_for_cancel {
                loop {
                    if let Err(error) = control.check() {
                        break Some(error);
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
            } else {
                control.check().err()
            };
            let cleanup = self.release();
            let outcome = OperationOutcome {
                state: if cause.is_some() {
                    OperationState::Cancelled
                } else {
                    OperationState::Completed
                },
                submission: if cause.is_some() {
                    Submission::PartialOrUnknown
                } else {
                    Submission::Complete
                },
                selected_backends: request.selected_backends(),
                cleanup,
                cursor_after: None,
                target_observed: false,
            };
            match cause {
                Some(error) => ToolResponse::with_error(outcome, ToolFailure::from_core(&error)),
                None => ToolResponse::success(outcome),
            }
        }

        fn release(&mut self) -> CleanupResult {
            if !self.initialized.load(Ordering::Acquire) {
                return CleanupResult::not_opened();
            }
            self.releases.fetch_add(1, Ordering::SeqCst);
            if let Some(gate) = self.release_gate.take() {
                let _ = gate.recv();
            }
            // Only this backend's worker thread mutates the failure counter.
            let failed = self.failures.load(Ordering::SeqCst) > 0;
            if failed {
                self.failures.fetch_sub(1, Ordering::SeqCst);
            }
            CleanupResult {
                keyboard: if failed {
                    ReleaseState::Failed
                } else {
                    ReleaseState::Released
                },
                mouse: ReleaseState::Released,
                errors: Vec::new(),
            }
        }
    }

    struct Harness {
        executor: Executor,
        entered: oneshot::Receiver<()>,
        actions: Arc<AtomicUsize>,
        releases: Arc<AtomicUsize>,
    }

    fn harness(
        allow: bool,
        wait: bool,
        failures: usize,
        gate: Option<std::sync::mpsc::Receiver<()>>,
        panic: bool,
    ) -> Harness {
        let actions = Arc::new(AtomicUsize::new(0));
        let releases = Arc::new(AtomicUsize::new(0));
        let (entered_tx, entered) = oneshot::channel();
        let (backend_actions, backend_releases) = (actions.clone(), releases.clone());
        let executor = Executor::with_backend(allow, move |initialized| Backend {
            initialized,
            actions: backend_actions,
            releases: backend_releases,
            failures: Arc::new(AtomicUsize::new(failures)),
            entered: Some(entered_tx),
            wait_for_cancel: wait,
            release_gate: gate,
            panic,
            _thread_affine: Rc::new(()),
        });
        Harness {
            executor,
            entered,
            actions,
            releases,
        }
    }

    #[tokio::test]
    async fn disabled_and_invalid_inputs_never_reach_backend() {
        let h = harness(false, false, 0, None, false);
        let response = h.executor.submit(request(), CancellationToken::new()).await;
        assert_eq!(response.error.unwrap().code, ErrorCode::InputDisabled);
        assert_eq!(response.data.unwrap().submission, Submission::None);
        let mut invalid = request();
        if let InputRequest::Key(params) = &mut invalid {
            params.keys.clear();
        }
        let response = h.executor.submit(invalid, CancellationToken::new()).await;
        assert_eq!(response.error.unwrap().code, ErrorCode::InvalidArguments);
        assert_eq!(h.actions.load(Ordering::SeqCst), 0);
        assert!(!h.executor.snapshot().device_initialized);
        let stop = h.executor.stop().await;
        assert_eq!(stop.data.unwrap().cleanup.keyboard, ReleaseState::NotOpened);
        assert_eq!(h.releases.load(Ordering::SeqCst), 0);
        h.executor.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn busy_is_immediate_and_concurrent_stop_cancels_without_duplicate_release() {
        let (release, gate) = std::sync::mpsc::channel();
        let h = harness(true, true, 0, Some(gate), false);
        let task_executor = h.executor.clone();
        let task = tokio::spawn(async move {
            task_executor
                .submit(request(), CancellationToken::new())
                .await
        });
        h.entered.await.unwrap();
        let busy = h.executor.submit(request(), CancellationToken::new()).await;
        assert_eq!(busy.error.unwrap().code, ErrorCode::Busy);
        let (stop_a, stop_b, ()) = tokio::join!(h.executor.stop(), h.executor.stop(), async {
            tokio::task::yield_now().await;
            release.send(()).unwrap();
        },);
        assert!(stop_a.ok && stop_b.ok);
        assert_eq!(
            task.await.unwrap().data.unwrap().state,
            OperationState::Cancelled
        );
        assert_eq!(h.actions.load(Ordering::SeqCst), 1);
        assert_eq!(h.releases.load(Ordering::SeqCst), 1);
        assert_eq!(h.executor.snapshot().worker_state, WorkerState::Idle);
        h.executor.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn dropping_submit_cancels_and_preserves_final_operation() {
        let h = harness(true, true, 0, None, false);
        let executor = h.executor.clone();
        let task =
            tokio::spawn(async move { executor.submit(request(), CancellationToken::new()).await });
        h.entered.await.unwrap();
        task.abort();
        let _ = task.await;
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let notified = h.executor.inner.shared.changed.notified();
                tokio::pin!(notified);
                notified.as_mut().enable();
                if h.executor.snapshot().worker_state == WorkerState::Idle {
                    break;
                }
                notified.await;
            }
        })
        .await
        .unwrap();
        let snapshot = h.executor.snapshot();
        assert_eq!(
            snapshot.last_operation.unwrap().data.unwrap().state,
            OperationState::Cancelled
        );
        assert_eq!(h.releases.load(Ordering::SeqCst), 1);
        h.executor.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn failed_final_cleanup_faults_until_stop_retries_on_existing_backend() {
        let h = harness(true, false, 1, None, false);
        let response = h.executor.submit(request(), CancellationToken::new()).await;
        assert_eq!(response.error.unwrap().code, ErrorCode::CleanupFailed);
        assert_eq!(response.data.unwrap().submission, Submission::Complete);
        assert_eq!(h.executor.snapshot().worker_state, WorkerState::Faulted);
        let rejected = h.executor.submit(request(), CancellationToken::new()).await;
        assert_eq!(rejected.error.unwrap().code, ErrorCode::CleanupFailed);
        assert_eq!(h.actions.load(Ordering::SeqCst), 1);
        assert!(h.executor.stop().await.ok);
        assert_eq!(h.releases.load(Ordering::SeqCst), 2);
        assert_eq!(h.executor.snapshot().worker_state, WorkerState::Idle);
        h.executor.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn shutdown_waits_for_cleanup_before_reporting_closed() {
        let (release, gate) = std::sync::mpsc::channel();
        // A blocking release, not scheduler timing, defines the shutdown gate.
        let h = harness(true, true, 0, Some(gate), false);
        let executor = h.executor.clone();
        let operation =
            tokio::spawn(async move { executor.submit(request(), CancellationToken::new()).await });
        h.entered.await.unwrap();
        let executor = h.executor.clone();
        let shutdown = tokio::spawn(async move { executor.shutdown().await });
        tokio::time::timeout(Duration::from_secs(2), async {
            while h.releases.load(Ordering::SeqCst) == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(!shutdown.is_finished());
        assert_ne!(h.executor.snapshot().worker_state, WorkerState::Closed);
        release.send(()).unwrap();
        operation.await.unwrap();
        shutdown.await.unwrap().unwrap();
        assert_eq!(h.executor.snapshot().worker_state, WorkerState::Closed);
        assert!(h.releases.load(Ordering::SeqCst) >= 1);
    }

    #[tokio::test]
    async fn worker_panic_closes_admission_and_surfaces_shutdown_failure() {
        let h = harness(true, false, 0, None, true);
        let response = h.executor.submit(request(), CancellationToken::new()).await;
        assert_eq!(response.error.unwrap().code, ErrorCode::WorkerClosed);
        let response = h.executor.submit(request(), CancellationToken::new()).await;
        assert_eq!(response.error.unwrap().code, ErrorCode::WorkerClosed);
        assert!(h.executor.shutdown().await.is_err());
        assert_eq!(h.executor.snapshot().worker_state, WorkerState::Closed);
    }

    #[tokio::test]
    async fn earlier_cleanup_error_does_not_fault_after_final_release_succeeds() {
        struct RecoveredBackend {
            initialized: Arc<AtomicBool>,
        }
        impl InputBackend for RecoveredBackend {
            fn execute(
                &mut self,
                request: InputRequest,
                _control: &OperationControl,
            ) -> ToolResponse<OperationOutcome> {
                self.initialized.store(true, Ordering::Release);
                let mut cleanup = self.release();
                cleanup.errors.push(crate::params::NativeError::from_core(
                    &logi_vhid::Error::Cancelled,
                ));
                ToolResponse::with_error(
                    OperationOutcome {
                        state: OperationState::Cancelled,
                        submission: Submission::PartialOrUnknown,
                        selected_backends: request.selected_backends(),
                        cleanup,
                        cursor_after: None,
                        target_observed: false,
                    },
                    ToolFailure::from_core(&logi_vhid::Error::Cancelled),
                )
            }
            fn release(&mut self) -> CleanupResult {
                CleanupResult {
                    keyboard: ReleaseState::Released,
                    mouse: ReleaseState::Released,
                    errors: Vec::new(),
                }
            }
        }
        let executor = Executor::with_backend(true, |initialized| RecoveredBackend { initialized });
        let response = executor.submit(request(), CancellationToken::new()).await;
        assert_eq!(response.error.unwrap().code, ErrorCode::Cancelled);
        assert!(!response.data.unwrap().cleanup.errors.is_empty());
        assert_eq!(executor.snapshot().worker_state, WorkerState::Idle);
        let response = executor.submit(request(), CancellationToken::new()).await;
        assert_ne!(response.error.unwrap().code, ErrorCode::CleanupFailed);
        executor.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn stop_during_synchronous_open_never_claims_inputs_are_not_opened() {
        struct OpeningBackend {
            initialized: Arc<AtomicBool>,
            entered: Option<oneshot::Sender<()>>,
            finish_open: std::sync::mpsc::Receiver<()>,
        }
        impl InputBackend for OpeningBackend {
            fn execute(
                &mut self,
                request: InputRequest,
                control: &OperationControl,
            ) -> ToolResponse<OperationOutcome> {
                self.entered.take().unwrap().send(()).unwrap();
                self.finish_open.recv().unwrap();
                self.initialized.store(true, Ordering::Release);
                let cause = control.check().unwrap_err();
                ToolResponse::with_error(
                    OperationOutcome {
                        state: OperationState::Cancelled,
                        submission: Submission::None,
                        selected_backends: request.selected_backends(),
                        cleanup: self.release(),
                        cursor_after: None,
                        target_observed: false,
                    },
                    ToolFailure::from_core(&cause),
                )
            }
            fn release(&mut self) -> CleanupResult {
                CleanupResult {
                    keyboard: ReleaseState::Released,
                    mouse: ReleaseState::Released,
                    errors: Vec::new(),
                }
            }
        }
        let (entered_tx, entered) = oneshot::channel();
        let (finish_open, open_gate) = std::sync::mpsc::channel();
        let executor = Executor::with_backend(true, move |initialized| OpeningBackend {
            initialized,
            entered: Some(entered_tx),
            finish_open: open_gate,
        });
        let operation_executor = executor.clone();
        let operation = tokio::spawn(async move {
            operation_executor
                .submit(request(), CancellationToken::new())
                .await
        });
        entered.await.unwrap();
        assert!(!executor.snapshot().device_initialized);
        let response = executor.stop().await;
        let outcome = response.data.unwrap();
        assert_eq!(outcome.state, OperationState::Stopping);
        assert_eq!(outcome.cleanup.keyboard, ReleaseState::Pending);
        assert_eq!(outcome.cleanup.mouse, ReleaseState::Pending);
        finish_open.send(()).unwrap();
        operation.await.unwrap();
        assert_eq!(executor.snapshot().worker_state, WorkerState::Idle);
        executor.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn slow_cleanup_returns_pending_then_commits_shared_stop_result() {
        let (release, gate) = std::sync::mpsc::channel();
        let h = harness(true, true, 0, Some(gate), false);
        let executor = h.executor.clone();
        let task =
            tokio::spawn(async move { executor.submit(request(), CancellationToken::new()).await });
        h.entered.await.unwrap();
        let response = tokio::time::timeout(Duration::from_secs(3), h.executor.stop())
            .await
            .unwrap();
        assert!(response.ok);
        let outcome = response.data.unwrap();
        assert_eq!(outcome.state, OperationState::Stopping);
        assert_eq!(outcome.cleanup.keyboard, ReleaseState::Pending);
        assert_eq!(h.executor.snapshot().worker_state, WorkerState::Stopping);
        let rejected = h.executor.submit(request(), CancellationToken::new()).await;
        assert_eq!(rejected.error.unwrap().code, ErrorCode::Busy);
        release.send(()).unwrap();
        task.await.unwrap();
        let snapshot = h.executor.snapshot();
        assert_eq!(snapshot.worker_state, WorkerState::Idle);
        let stopped = snapshot.last_operation.unwrap();
        assert!(stopped.ok);
        assert_eq!(
            stopped.data.unwrap().cleanup.keyboard,
            ReleaseState::Released
        );
        assert_eq!(h.releases.load(Ordering::SeqCst), 1);
        h.executor.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn shutdown_propagates_release_failure_without_erasing_operation_evidence() {
        let h = harness(true, false, 2, None, false);
        let response = h.executor.submit(request(), CancellationToken::new()).await;
        assert_eq!(response.error.unwrap().code, ErrorCode::CleanupFailed);
        assert!(h.executor.shutdown().await.is_err());
        let snapshot = h.executor.snapshot();
        assert_eq!(snapshot.worker_state, WorkerState::Closed);
        let operation = snapshot.last_operation.unwrap();
        assert_eq!(operation.error.unwrap().code, ErrorCode::CleanupFailed);
        assert_eq!(operation.data.unwrap().submission, Submission::Complete);
    }

    #[test]
    fn cancellation_and_deadline_are_core_errors_without_native_work() {
        let request_ct = CancellationToken::new();
        let control = OperationControl {
            request_ct: request_ct.clone(),
            stop_ct: CancellationToken::new(),
            shutdown_ct: CancellationToken::new(),
            deadline: Instant::now() - Duration::from_secs(1),
        };
        assert!(matches!(control.check(), Err(logi_vhid::Error::TimedOut)));
        request_ct.cancel();
        assert!(matches!(control.check(), Err(logi_vhid::Error::Cancelled)));
    }
}
