//! The only production input backend. All owned native handles stay on the worker.

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use logi_vhid::{Action, Error, LogitechVirtualHid, OpenOptions, ReleaseReport, Sequence};

use crate::executor::{InputBackend, OperationControl};
use crate::params::*;
use crate::windows::{InputMutex, TargetGuard};

pub(crate) fn probe_driver() -> DriverStatus {
    let result = LogitechVirtualHid::open_with_options(OpenOptions {
        keyboard: false,
        mouse: false,
    })
    .and_then(|device| device.device_info());
    match result {
        Ok(info) => {
            let pid_gate_active = info.pid_gate_active();
            DriverStatus {
                state: DriverState::Available,
                info: Some(DeviceInfoDto {
                    control_path: info.control_path,
                    keyboard_path: info.keyboard_path,
                    mouse_path: info.mouse_path,
                    registered_pids: info.registered_pids,
                    pid_gate_active,
                }),
                error: None,
            }
        }
        Err(error) => DriverStatus {
            state: if matches!(error, Error::NotFound(_)) {
                DriverState::NotFound
            } else {
                DriverState::Error
            },
            info: None,
            error: Some(NativeError::from_core(&error)),
        },
    }
}

pub(crate) struct NativeBackend {
    // Declaration order closes the device before releasing the cooperating lock.
    device: Option<LogitechVirtualHid>,
    mutex: Option<InputMutex>,
    initialized: Arc<AtomicBool>,
}

impl NativeBackend {
    pub(crate) fn new(initialized: Arc<AtomicBool>) -> Self {
        Self {
            device: None,
            mutex: None,
            initialized,
        }
    }

    fn initialize(
        &mut self,
        checkpoint: &dyn Fn() -> logi_vhid::Result<()>,
        initialization_cleanup: &mut Option<CleanupResult>,
    ) -> Result<(), ToolFailure> {
        checkpoint().map_err(|error| ToolFailure::from_core(&error))?;
        if self.device.is_some() {
            return Ok(());
        }
        if self.mutex.is_none() {
            let (mutex, _abandoned) = InputMutex::acquire()?;
            self.mutex = Some(mutex);
        }
        checkpoint().map_err(|error| ToolFailure::from_core(&error))?;
        // Open's existing PnP waits cannot be interrupted. Store the handle before
        // checking cancellation so the outer finally can release an opened device.
        self.device =
            Some(LogitechVirtualHid::open().map_err(|error| ToolFailure::from_core(&error))?);
        self.initialized.store(true, Ordering::Release);
        checkpoint().map_err(|error| ToolFailure::from_core(&error))?;
        // Required for every first open, including abandoned mutex acquisition.
        let cleanup = self.release();
        if cleanup.has_failed() {
            *initialization_cleanup = Some(cleanup);
            return Err(ToolFailure::new(
                ErrorCode::CleanupFailed,
                "Initial neutral keyboard/mouse reports failed; stop must release them before new input.",
            ));
        }
        Ok(())
    }

    fn perform(
        &mut self,
        request: InputRequest,
        control: &OperationControl,
        submitted: &mut bool,
        cleanup_errors: &mut Vec<NativeError>,
        initialization_cleanup: &mut Option<CleanupResult>,
    ) -> Result<(), ToolFailure> {
        control
            .check()
            .map_err(|error| ToolFailure::from_core(&error))?;
        if request.is_empty() {
            return Ok(());
        }
        let guard = TargetGuard::new(request.target())?;
        // Resolve all requested coordinates before acquiring/opening input. Never
        // use core's clamping to silently change a requested click or drag point.
        let prepared = match request {
            InputRequest::Mouse(params) => {
                Prepared::Mouse(PreparedMouse::new(params.action, &guard)?)
            }
            InputRequest::Key(params) => Prepared::Key(params),
            InputRequest::Text(params) => Prepared::Text(params),
        };
        let safety_failure = RefCell::new(None);
        let checkpoint = || {
            control.check()?;
            if let Err(error) = guard.check() {
                *safety_failure.borrow_mut() = Some(error);
                return Err(Error::Protocol(
                    "foreground or input desktop changed".to_owned(),
                ));
            }
            Ok(())
        };
        let result = (|| {
            self.initialize(&checkpoint, initialization_cleanup)?;
            checkpoint().map_err(|error| ToolFailure::from_core(&error))?;
            let device = self.device.as_mut().expect("successful initialization");
            match prepared {
                Prepared::Key(params) => {
                    let mut sequence = Sequence::new();
                    for key in &params.keys {
                        sequence.actions.push(Action::KeyDown(key.usage()));
                    }
                    sequence.actions.push(Action::Sleep(Duration::from_millis(
                        params.hold_ms.unwrap_or(20),
                    )));
                    for key in params.keys.iter().rev() {
                        sequence.actions.push(Action::KeyUp(key.usage()));
                    }
                    run(device, sequence, &checkpoint, submitted, cleanup_errors)
                }
                Prepared::Text(params) => run(
                    device,
                    Sequence::new().push(Action::TypeText {
                        text: params.text,
                        mode: params.mode.core(),
                    }),
                    &checkpoint,
                    submitted,
                    cleanup_errors,
                ),
                Prepared::Mouse(mouse) => {
                    mouse.run(device, &guard, &checkpoint, submitted, cleanup_errors)
                }
            }
        })();
        // Do not guess service causes by parsing core's interruption string.
        match safety_failure.into_inner() {
            Some(error) => Err(error),
            None => result,
        }
    }
}

impl InputBackend for NativeBackend {
    fn execute(
        &mut self,
        request: InputRequest,
        control: &OperationControl,
    ) -> ToolResponse<OperationOutcome> {
        let mut outcome = OperationOutcome::rejected(request.selected_backends());
        let empty = request.is_empty();
        let mut submitted = false;
        let mut earlier_cleanup_errors = Vec::new();
        let mut initialization_cleanup = None;
        let result = self.perform(
            request,
            control,
            &mut submitted,
            &mut earlier_cleanup_errors,
            &mut initialization_cleanup,
        );
        // Initial neutral failure is already the final cleanup attempt. Do not
        // disguise it by automatically retrying the same reports.
        let mut cleanup = initialization_cleanup.unwrap_or_else(|| self.release());
        earlier_cleanup_errors.append(&mut cleanup.errors);
        cleanup.errors = earlier_cleanup_errors;
        outcome.cleanup = cleanup;
        outcome.cursor_after = logi_vhid::cursor_pos().ok().map(|point| PointDto {
            x: point.x,
            y: point.y,
        });
        finish_operation(outcome, result, submitted, empty)
    }

    fn release(&mut self) -> CleanupResult {
        self.device
            .as_mut()
            .map_or_else(CleanupResult::not_opened, |device| {
                release_result(device.release_all_inputs())
            })
    }
}

fn finish_operation(
    mut outcome: OperationOutcome,
    result: Result<(), ToolFailure>,
    submitted: bool,
    empty: bool,
) -> ToolResponse<OperationOutcome> {
    outcome.submission = if result.is_ok() && !empty {
        Submission::Complete
    } else if submitted {
        Submission::PartialOrUnknown
    } else {
        Submission::None
    };
    if outcome.cleanup.has_failed() {
        outcome.state = OperationState::Failed;
        return ToolResponse::with_error(outcome, ToolFailure::new(
            ErrorCode::CleanupFailed,
            "Keyboard/mouse release failed; no further input is accepted until stop releases it.",
        ));
    }
    match result {
        Ok(()) => {
            outcome.state = OperationState::Completed;
            ToolResponse::success(outcome)
        }
        Err(error) => {
            outcome.state = match error.code {
                ErrorCode::Cancelled => OperationState::Cancelled,
                ErrorCode::TimedOut => OperationState::TimedOut,
                _ if !submitted => OperationState::Rejected,
                _ => OperationState::Failed,
            };
            ToolResponse::with_error(outcome, error)
        }
    }
}

fn release_result(report: ReleaseReport) -> CleanupResult {
    let mut errors = Vec::new();
    let mut convert = |result: Option<logi_vhid::Result<()>>| match result {
        None => ReleaseState::NotOpened,
        Some(Ok(())) => ReleaseState::Released,
        Some(Err(error)) => {
            errors.push(NativeError::from_core(&error));
            ReleaseState::Failed
        }
    };
    let keyboard = convert(report.keyboard);
    let mouse = convert(report.mouse);
    CleanupResult {
        keyboard,
        mouse,
        errors,
    }
}

enum Prepared {
    Key(KeyParams),
    Text(TextParams),
    Mouse(PreparedMouse),
}

enum PreparedMouse {
    Relative {
        dx: i16,
        dy: i16,
    },
    Move {
        at: PointDto,
        mode: PositionMode,
    },
    Click {
        at: PointDto,
        mode: PositionMode,
        button: Button,
        clicks: u8,
        hold: u64,
    },
    Scroll {
        at: Option<PointDto>,
        mode: PositionMode,
        delta: i8,
    },
    Drag {
        from: PointDto,
        to: PointDto,
        mode: PositionMode,
        button: Button,
        hold: u64,
    },
}

impl PreparedMouse {
    fn new(action: MouseAction, guard: &TargetGuard) -> Result<Self, ToolFailure> {
        Ok(match action {
            MouseAction::MoveRelative { dx, dy } => Self::Relative { dx, dy },
            MouseAction::MoveTo { at, mode } => Self::Move {
                at: guard.resolve_point(&at)?,
                mode: mode.unwrap_or_default(),
            },
            MouseAction::Click {
                at,
                button,
                clicks,
                hold_ms,
                mode,
            } => {
                let at = guard.resolve_point(&at)?;
                guard.hit_test(at)?;
                Self::Click {
                    at,
                    mode: mode.unwrap_or_default(),
                    button: button.unwrap_or_default(),
                    clicks: clicks.unwrap_or(1),
                    hold: hold_ms.unwrap_or(20),
                }
            }
            MouseAction::Scroll { delta, at, mode } => {
                let at = at
                    .as_ref()
                    .map(|point| guard.resolve_point(point))
                    .transpose()?;
                guard.hit_test(match at {
                    Some(point) => point,
                    None => cursor()?,
                })?;
                Self::Scroll {
                    at,
                    mode: mode.unwrap_or_default(),
                    delta,
                }
            }
            MouseAction::Drag {
                from,
                to,
                button,
                hold_ms,
                mode,
            } => {
                let from = guard.resolve_point(&from)?;
                let to = guard.resolve_point(&to)?;
                guard.hit_test(from)?;
                guard.hit_test(to)?;
                Self::Drag {
                    from,
                    to,
                    mode: mode.unwrap_or_default(),
                    button: button.unwrap_or_default(),
                    hold: hold_ms.unwrap_or(20),
                }
            }
        })
    }

    fn run(
        self,
        device: &mut LogitechVirtualHid,
        guard: &TargetGuard,
        checkpoint: &dyn Fn() -> logi_vhid::Result<()>,
        submitted: &mut bool,
        cleanup_errors: &mut Vec<NativeError>,
    ) -> Result<(), ToolFailure> {
        match self {
            Self::Relative { dx, dy } => {
                mouse_available(device)?;
                run(
                    device,
                    Sequence::new().move_relative(i32::from(dx), i32::from(dy)),
                    checkpoint,
                    submitted,
                    cleanup_errors,
                )
            }
            Self::Move { at, mode } => {
                position(device, at, mode, checkpoint, submitted, cleanup_errors)
            }
            Self::Click {
                at,
                mode,
                button,
                clicks,
                hold,
            } => {
                position(device, at, mode, checkpoint, submitted, cleanup_errors)?;
                for index in 0..clicks {
                    if index != 0 {
                        wait(Duration::from_millis(40), checkpoint)?;
                    }
                    checked_hit(guard, at, checkpoint)?;
                    run(
                        device,
                        Sequence::new()
                            .push(Action::MouseDown(button.core()))
                            .sleep(Duration::from_millis(hold))
                            .push(Action::MouseUp(button.core())),
                        checkpoint,
                        submitted,
                        cleanup_errors,
                    )?;
                }
                Ok(())
            }
            Self::Scroll { at, mode, delta } => {
                if let Some(at) = at {
                    position(device, at, mode, checkpoint, submitted, cleanup_errors)?;
                }
                let point = match at {
                    Some(point) => point,
                    None => cursor()?,
                };
                checked_hit(guard, point, checkpoint)?;
                run(
                    device,
                    Sequence::new().push(Action::Wheel(delta)),
                    checkpoint,
                    submitted,
                    cleanup_errors,
                )
            }
            Self::Drag {
                from,
                to,
                mode,
                button,
                hold,
            } => {
                position(device, from, mode, checkpoint, submitted, cleanup_errors)?;
                checked_hit(guard, from, checkpoint)?;
                run(
                    device,
                    Sequence::new()
                        .push(Action::MouseDown(button.core()))
                        .sleep(Duration::from_millis(hold)),
                    checkpoint,
                    submitted,
                    cleanup_errors,
                )?;
                position(device, to, mode, checkpoint, submitted, cleanup_errors)?;
                checked_hit(guard, to, checkpoint)?;
                run(
                    device,
                    Sequence::new().push(Action::MouseUp(button.core())),
                    checkpoint,
                    submitted,
                    cleanup_errors,
                )
            }
        }
    }
}

fn mouse_available(device: &LogitechVirtualHid) -> Result<(), ToolFailure> {
    device
        .ensure_mouse_available()
        .map_err(|error| match error {
            Error::Protocol(_) => ToolFailure {
                code: ErrorCode::PidGateActive,
                message: "Registered PIDs currently gate driver mouse movement.".to_owned(),
                native: Some(NativeError::from_core(&error)),
            },
            _ => ToolFailure::from_core(&error),
        })
}

fn run(
    device: &mut LogitechVirtualHid,
    sequence: Sequence,
    checkpoint: &dyn Fn() -> logi_vhid::Result<()>,
    submitted: &mut bool,
    cleanup_errors: &mut Vec<NativeError>,
) -> Result<(), ToolFailure> {
    checkpoint().map_err(|error| ToolFailure::from_core(&error))?;
    *submitted = true;
    sequence
        .run_controlled(device, checkpoint)
        .map_err(|failure| {
            for error in [failure.cleanup.keyboard, failure.cleanup.mouse]
                .into_iter()
                .flatten()
                .filter_map(Result::err)
            {
                cleanup_errors.push(NativeError::from_core(&error));
            }
            ToolFailure::from_core(&failure.cause)
        })
}

fn position(
    device: &mut LogitechVirtualHid,
    at: PointDto,
    mode: PositionMode,
    checkpoint: &dyn Fn() -> logi_vhid::Result<()>,
    submitted: &mut bool,
    cleanup_errors: &mut Vec<NativeError>,
) -> Result<(), ToolFailure> {
    if matches!(mode, PositionMode::Driver) {
        mouse_available(device)?;
    }
    run(
        device,
        Sequence::new().move_absolute(at.x, at.y, mode.core()),
        checkpoint,
        submitted,
        cleanup_errors,
    )
    .map_err(|mut error| {
        if error
            .native
            .as_ref()
            .is_some_and(|native| matches!(native.kind, NativeKind::Protocol))
        {
            error.code = ErrorCode::PositionNotReached;
        }
        error
    })?;
    let start = Instant::now();
    loop {
        checkpoint().map_err(|error| ToolFailure::from_core(&error))?;
        let observed = cursor()?;
        if observed.x == at.x && observed.y == at.y {
            return Ok(());
        }
        if matches!(mode, PositionMode::Driver) || start.elapsed() >= Duration::from_millis(200) {
            return Err(ToolFailure::new(ErrorCode::PositionNotReached,
                "The cursor did not reach the requested physical pixel; no click or scroll will follow."));
        }
        wait(Duration::from_millis(10), checkpoint)?;
    }
}

fn cursor() -> Result<PointDto, ToolFailure> {
    logi_vhid::cursor_pos()
        .map(|point| PointDto {
            x: point.x,
            y: point.y,
        })
        .map_err(|error| ToolFailure::from_core(&error))
}

fn checked_hit(
    guard: &TargetGuard,
    at: PointDto,
    checkpoint: &dyn Fn() -> logi_vhid::Result<()>,
) -> Result<(), ToolFailure> {
    checkpoint().map_err(|error| ToolFailure::from_core(&error))?;
    let actual = cursor()?;
    if actual.x != at.x || actual.y != at.y {
        return Err(ToolFailure::new(
            ErrorCode::PositionNotReached,
            "The cursor moved before input submission; observe the desktop again.",
        ));
    }
    guard.hit_test(actual)
}

fn wait(
    duration: Duration,
    checkpoint: &dyn Fn() -> logi_vhid::Result<()>,
) -> Result<(), ToolFailure> {
    let start = Instant::now();
    loop {
        checkpoint().map_err(|error| ToolFailure::from_core(&error))?;
        let remaining = duration.saturating_sub(start.elapsed());
        if remaining.is_zero() {
            return Ok(());
        }
        std::thread::sleep(remaining.min(Duration::from_millis(10)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_submission_is_not_downgraded_when_final_release_fails() {
        let mut outcome = OperationOutcome::rejected(vec![BackendKind::GhubHidKeyboard]);
        outcome.cleanup.keyboard = ReleaseState::Failed;
        outcome.cleanup.mouse = ReleaseState::Released;
        let result = finish_operation(outcome, Ok(()), true, false);
        assert!(!result.ok);
        assert_eq!(result.error.unwrap().code, ErrorCode::CleanupFailed);
        let outcome = result.data.unwrap();
        assert_eq!(outcome.state, OperationState::Failed);
        assert_eq!(outcome.submission, Submission::Complete);
        assert!(!outcome.target_observed);
    }

    #[test]
    fn earlier_cleanup_error_does_not_fault_successful_final_release() {
        let mut outcome = OperationOutcome::rejected(vec![]);
        outcome.cleanup.keyboard = ReleaseState::Released;
        outcome.cleanup.mouse = ReleaseState::Released;
        outcome
            .cleanup
            .errors
            .push(NativeError::from_core(&Error::Protocol(
                "earlier cleanup failed".to_owned(),
            )));
        let result = finish_operation(outcome, Ok(()), true, false);
        assert!(result.ok);
        let outcome = result.data.unwrap();
        assert_eq!(outcome.state, OperationState::Completed);
        assert_eq!(outcome.cleanup.errors.len(), 1);
    }

    #[test]
    fn rejected_and_cancelled_operations_report_distinct_submission_states() {
        let rejected = finish_operation(
            OperationOutcome::rejected(vec![]),
            Err(ToolFailure::new(ErrorCode::TargetChanged, "changed target")),
            false,
            false,
        );
        let outcome = rejected.data.unwrap();
        assert_eq!(outcome.state, OperationState::Rejected);
        assert_eq!(outcome.submission, Submission::None);
        let cancelled = finish_operation(
            OperationOutcome::rejected(vec![]),
            Err(ToolFailure::from_core(&Error::Cancelled)),
            true,
            false,
        );
        let outcome = cancelled.data.unwrap();
        assert_eq!(outcome.state, OperationState::Cancelled);
        assert_eq!(outcome.submission, Submission::PartialOrUnknown);
    }
}
