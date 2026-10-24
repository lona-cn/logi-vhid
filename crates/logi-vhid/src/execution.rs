//! Cooperative execution and explicit input release results.

use std::fmt;
use std::time::{Duration, Instant};

use crate::device::ReportSink;
use crate::keyboard::KeyboardState;
use crate::mouse::MouseState;
use crate::{Error, LogitechVirtualHid, Result};

/// Results of attempting to release each selected input category.
///
/// `None` means the category was not opened. Success describes this object's
/// report submission, not acknowledgment from the foreground application.
#[derive(Debug)]
pub struct ReleaseReport {
    pub keyboard: Option<Result<()>>,
    pub mouse: Option<Result<()>>,
}

/// An execution error together with both baseline-preserving cleanup results.
#[derive(Debug)]
pub struct ExecutionFailure {
    pub cause: Error,
    pub cleanup: ReleaseReport,
}

impl fmt::Display for ExecutionFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.cause)?;
        if let Some(Err(error)) = &self.cleanup.keyboard {
            write!(f, "; keyboard cleanup failed: {error}")?;
        }
        if let Some(Err(error)) = &self.cleanup.mouse {
            write!(f, "; mouse cleanup failed: {error}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ExecutionFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.cause)
    }
}

#[derive(Clone, Copy)]
pub(crate) struct ExecutionContext<'a> {
    checkpoint: Option<&'a dyn Fn() -> Result<()>>,
}

impl<'a> ExecutionContext<'a> {
    pub(crate) fn uncontrolled() -> Self {
        Self { checkpoint: None }
    }

    pub(crate) fn controlled(checkpoint: &'a dyn Fn() -> Result<()>) -> Self {
        Self {
            checkpoint: Some(checkpoint),
        }
    }

    pub(crate) fn check(&self) -> Result<()> {
        match self.checkpoint {
            Some(checkpoint) => checkpoint(),
            None => Ok(()),
        }
    }

    pub(crate) fn wait(&self, duration: Duration) -> Result<()> {
        if self.checkpoint.is_none() {
            std::thread::sleep(duration);
            return Ok(());
        }
        let start = Instant::now();
        loop {
            self.check()?;
            let remaining = duration.saturating_sub(start.elapsed());
            if remaining.is_zero() {
                return Ok(());
            }
            std::thread::sleep(remaining.min(Duration::from_millis(10)));
        }
    }
}

pub(crate) struct ControlledSink<'a, S> {
    pub(crate) sink: &'a S,
    pub(crate) control: ExecutionContext<'a>,
}

impl<S: ReportSink> ReportSink for ControlledSink<'_, S> {
    fn send_report(&self, code: u32, report: &[u8; 8]) -> Result<()> {
        self.control.check()?;
        self.sink.send_report(code, report)
    }

    fn send_cleanup_report(&self, code: u32, report: &[u8; 8]) -> Result<()> {
        self.sink.send_cleanup_report(code, report)
    }
}

pub(crate) fn release_inputs(
    sink: &impl ReportSink,
    keyboard: &mut KeyboardState,
    mouse: &mut MouseState,
    enabled: (bool, bool),
) -> ReleaseReport {
    let keyboard = enabled.0.then(|| keyboard.release_all(sink));
    let mouse = enabled.1.then(|| mouse.release_all(sink));
    ReleaseReport { keyboard, mouse }
}

impl LogitechVirtualHid {
    /// Submit neutral reports for every opened category, even if one fails.
    ///
    /// Cleanup is not gated by cooperative cancellation. Each local state is
    /// cleared only after its neutral report succeeds. Synchronous I/O may block.
    pub fn release_all_inputs(&mut self) -> ReleaseReport {
        release_inputs(
            &self.device,
            &mut self.keyboard,
            &mut self.mouse,
            (self.keyboard_enabled, self.mouse_enabled),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keyboard::{hid, KeyboardState};
    use crate::mouse::MouseState;
    use crate::protocol::{IOCTL_KEYBOARD_REPORT, IOCTL_MOUSE_REPORT};
    use crate::test_support::Reports;
    use crate::MouseButton;

    #[test]
    fn mouse_only_release_sends_neutral_report_and_does_not_require_keyboard() {
        let reports = Reports::default();
        let mut keyboard = KeyboardState::new();
        let mut mouse = MouseState::new();
        mouse.button_down(&reports, MouseButton::Right).unwrap();
        let released = release_inputs(&reports, &mut keyboard, &mut mouse, (false, true));
        assert!(released.keyboard.is_none());
        assert!(matches!(released.mouse, Some(Ok(()))));
        assert_eq!(
            reports.accepted(),
            vec![
                (IOCTL_MOUSE_REPORT, [2, 0, 0, 0, 0, 0, 0, 0]),
                (IOCTL_MOUSE_REPORT, [0; 8]),
            ]
        );
    }

    #[test]
    fn release_attempts_mouse_after_keyboard_failure_and_keeps_failed_state_for_retry() {
        let reports = Reports::default();
        let mut keyboard = KeyboardState::new();
        let mut mouse = MouseState::new();
        keyboard.key_down(&reports, hid::A).unwrap();
        mouse.button_down(&reports, MouseButton::Left).unwrap();
        reports.fail_on(&[3]);
        let released = release_inputs(&reports, &mut keyboard, &mut mouse, (true, true));
        assert!(matches!(released.keyboard, Some(Err(_))));
        assert!(matches!(released.mouse, Some(Ok(()))));
        assert_eq!(reports.last(), [0; 8]);
        keyboard.key_down(&reports, hid::B).unwrap();
        assert_eq!(reports.last(), [0, 0, hid::A, hid::B, 0, 0, 0, 0]);
        mouse.move_relative(&reports, 1, 0).unwrap();
        assert_eq!(reports.last(), [0, 0, 1, 0, 0, 0, 0, 0]);
        let retried = release_inputs(&reports, &mut keyboard, &mut mouse, (true, true));
        assert!(matches!(retried.keyboard, Some(Ok(()))));
        assert!(matches!(retried.mouse, Some(Ok(()))));
        assert_eq!(
            &reports.accepted()[5..],
            &[
                (IOCTL_KEYBOARD_REPORT, [0; 8]),
                (IOCTL_MOUSE_REPORT, [0; 8])
            ]
        );
    }
}
