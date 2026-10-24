//! Mouse report state and relative movement.

use std::time::Duration;

use crate::device::ReportSink;
use crate::error::Result;
use crate::execution::ExecutionContext;
use crate::protocol::{MouseReport, IOCTL_MOUSE_REPORT};

/// Virtual mouse buttons; held state is tracked per [`crate::LogitechVirtualHid`]
/// object, not read from the physical mouse.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MouseButton {
    Left = 0,
    Right = 1,
    Middle = 2,
    Back = 3,
    Forward = 4,
}

impl MouseButton {
    pub(crate) fn bit(self) -> u16 {
        1u16 << (self as u16)
    }
}

#[derive(Clone, Copy)]
pub(crate) struct MouseState {
    buttons: u16,
}

impl MouseState {
    pub(crate) fn new() -> Self {
        Self { buttons: 0 }
    }

    pub(crate) fn move_relative(
        &mut self,
        device: &impl ReportSink,
        dx: i32,
        dy: i32,
    ) -> Result<()> {
        self.move_relative_with_control(device, dx, dy, ExecutionContext::uncontrolled())
    }

    pub(crate) fn move_relative_with_control(
        &mut self,
        device: &impl ReportSink,
        dx: i32,
        dy: i32,
        control: ExecutionContext<'_>,
    ) -> Result<()> {
        if dx == 0 && dy == 0 {
            control.check()?;
            return self.send(device, 0, 0, 0, 0);
        }

        let mut remaining_x = dx;
        let mut remaining_y = dy;
        let mut first = true;
        loop {
            control.check()?;
            let step_x = next_chunk(&mut remaining_x);
            let step_y = next_chunk(&mut remaining_y);
            if step_x.is_none() && step_y.is_none() {
                break;
            }
            if !first {
                control.wait(Duration::from_millis(1))?;
            }
            self.send(device, step_x.unwrap_or(0), step_y.unwrap_or(0), 0, 0)?;
            first = false;
        }
        Ok(())
    }

    pub(crate) fn button_down(
        &mut self,
        device: &impl ReportSink,
        button: MouseButton,
    ) -> Result<()> {
        let next = Self {
            buttons: self.buttons | button.bit(),
        };
        next.send(device, 0, 0, 0, 0)?;
        *self = next;
        Ok(())
    }

    pub(crate) fn button_up(
        &mut self,
        device: &impl ReportSink,
        button: MouseButton,
    ) -> Result<()> {
        let next = Self {
            buttons: self.buttons & !button.bit(),
        };
        next.send(device, 0, 0, 0, 0)?;
        *self = next;
        Ok(())
    }

    pub(crate) fn retain_held(&mut self, other: &Self) {
        self.buttons &= other.buttons;
    }

    pub(crate) fn cleanup_since(&mut self, device: &impl ReportSink, initial: &Self) -> Result<()> {
        let mut next = *self;
        next.retain_held(initial);
        if next.buttons != self.buttons {
            next.send_cleanup(device)?;
            *self = next;
        }
        Ok(())
    }

    pub(crate) fn release_all(&mut self, device: &impl ReportSink) -> Result<()> {
        let next = Self::new();
        next.send_cleanup(device)?;
        *self = next;
        Ok(())
    }

    pub(crate) fn click(&mut self, device: &impl ReportSink, button: MouseButton) -> Result<()> {
        self.click_with_control(device, button, ExecutionContext::uncontrolled())
    }

    pub(crate) fn click_with_control(
        &mut self,
        device: &impl ReportSink,
        button: MouseButton,
        control: ExecutionContext<'_>,
    ) -> Result<()> {
        self.hold_button_with_control(device, button, |_| Ok(()), control)
    }

    pub(crate) fn hold_button(
        &mut self,
        device: &impl ReportSink,
        button: MouseButton,
        action: impl FnOnce(&mut Self) -> Result<()>,
    ) -> Result<()> {
        self.hold_button_with_control(device, button, action, ExecutionContext::uncontrolled())
    }

    pub(crate) fn hold_button_with_control(
        &mut self,
        device: &impl ReportSink,
        button: MouseButton,
        action: impl FnOnce(&mut Self) -> Result<()>,
        control: ExecutionContext<'_>,
    ) -> Result<()> {
        let initial = *self;
        let result = (|| {
            control.check()?;
            self.button_down(device, button)?;
            control.wait(Duration::from_millis(20))?;
            action(self)?;
            control.check()?;
            if initial.buttons & button.bit() == 0 {
                self.button_up(device, button)?;
            }
            Ok(())
        })();
        let cleanup = self.cleanup_since(device, &initial);
        result.and(cleanup)
    }

    pub(crate) fn wheel(&mut self, device: &impl ReportSink, delta: i8) -> Result<()> {
        self.send(device, 0, 0, delta, 0)
    }

    fn send(&self, device: &impl ReportSink, dx: i16, dy: i16, wheel: i8, pan: i8) -> Result<()> {
        let report = MouseReport {
            buttons: self.buttons,
            dx,
            dy,
            wheel,
            pan,
        };
        device.send_report(IOCTL_MOUSE_REPORT, &report.to_bytes())
    }

    fn send_cleanup(&self, device: &impl ReportSink) -> Result<()> {
        let report = MouseReport {
            buttons: self.buttons,
            dx: 0,
            dy: 0,
            wheel: 0,
            pan: 0,
        };
        device.send_cleanup_report(IOCTL_MOUSE_REPORT, &report.to_bytes())
    }
}

fn next_chunk(remaining: &mut i32) -> Option<i16> {
    if *remaining == 0 {
        return None;
    }
    let step = (*remaining).clamp(i16::MIN as i32, i16::MAX as i32) as i16;
    *remaining -= step as i32;
    Some(step)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct CancelAfterReport<'a> {
        reports: &'a crate::test_support::Reports,
        cancelled: &'a std::cell::Cell<bool>,
    }

    impl ReportSink for CancelAfterReport<'_> {
        fn send_report(&self, code: u32, report: &[u8; 8]) -> Result<()> {
            self.reports.send_report(code, report)?;
            self.cancelled.set(true);
            Ok(())
        }
    }

    #[test]
    fn controlled_movement_stops_after_first_successful_chunk() {
        let reports = crate::test_support::Reports::default();
        let cancelled = std::cell::Cell::new(false);
        let checkpoint = || {
            if cancelled.get() {
                Err(crate::Error::Cancelled)
            } else {
                Ok(())
            }
        };
        let control = crate::execution::ExecutionContext::controlled(&checkpoint);
        let raw = CancelAfterReport {
            reports: &reports,
            cancelled: &cancelled,
        };
        let sink = crate::execution::ControlledSink {
            sink: &raw,
            control,
        };
        let mut mouse = MouseState::new();
        let error = mouse
            .move_relative_with_control(&sink, 40_000, -40_000, control)
            .unwrap_err();
        assert!(matches!(error, crate::Error::Cancelled));
        assert_eq!(
            reports.accepted(),
            vec![(IOCTL_MOUSE_REPORT, [0, 0, 255, 127, 0, 128, 0, 0])]
        );
    }

    #[test]
    fn cancelled_temporary_button_is_released_but_existing_button_is_retained() {
        let reports = crate::test_support::Reports::default();
        let mut mouse = MouseState::new();
        mouse.button_down(&reports, MouseButton::Right).unwrap();
        let cancelled = std::cell::Cell::new(false);
        let checkpoint = || {
            if cancelled.get() {
                Err(crate::Error::Cancelled)
            } else {
                Ok(())
            }
        };
        let control = crate::execution::ExecutionContext::controlled(&checkpoint);
        let raw = CancelAfterReport {
            reports: &reports,
            cancelled: &cancelled,
        };
        let sink = crate::execution::ControlledSink {
            sink: &raw,
            control,
        };
        let error = mouse
            .hold_button_with_control(
                &sink,
                MouseButton::Left,
                |_| panic!("cancelled hold must not run its action"),
                control,
            )
            .unwrap_err();
        assert!(matches!(error, crate::Error::Cancelled));
        assert_eq!(
            reports.accepted(),
            vec![
                (IOCTL_MOUSE_REPORT, [2, 0, 0, 0, 0, 0, 0, 0]),
                (IOCTL_MOUSE_REPORT, [3, 0, 0, 0, 0, 0, 0, 0]),
                (IOCTL_MOUSE_REPORT, [2, 0, 0, 0, 0, 0, 0, 0]),
            ]
        );
        mouse.move_relative(&reports, 0, 0).unwrap();
        assert_eq!(reports.last()[0], 2);
    }

    #[test]
    fn cancelled_business_button_up_keeps_state_until_cleanup() {
        let reports = crate::test_support::Reports::default();
        let mut mouse = MouseState::new();
        mouse.button_down(&reports, MouseButton::Left).unwrap();
        let checkpoint = || Err(crate::Error::Cancelled);
        let control = crate::execution::ExecutionContext::controlled(&checkpoint);
        let sink = crate::execution::ControlledSink {
            sink: &reports,
            control,
        };
        assert!(matches!(
            mouse.button_up(&sink, MouseButton::Left),
            Err(crate::Error::Cancelled)
        ));
        mouse.move_relative(&reports, 0, 0).unwrap();
        assert_eq!(reports.last()[0], 1);
        mouse.release_all(&sink).unwrap();
        assert_eq!(reports.last(), [0; 8]);
    }

    #[test]
    fn failed_full_release_preserves_state_and_success_always_sends_neutral() {
        let reports = crate::test_support::Reports::default();
        let mut mouse = MouseState::new();
        mouse.button_down(&reports, MouseButton::Left).unwrap();
        reports.fail_on(&[2]);
        assert!(mouse.release_all(&reports).is_err());
        mouse.move_relative(&reports, 0, 0).unwrap();
        assert_eq!(reports.last()[0], 1);
        mouse.release_all(&reports).unwrap();
        mouse.release_all(&reports).unwrap();
        assert_eq!(
            &reports.accepted()[2..],
            &[(IOCTL_MOUSE_REPORT, [0; 8]), (IOCTL_MOUSE_REPORT, [0; 8])]
        );
        mouse.move_relative(&reports, 0, 0).unwrap();
        assert_eq!(reports.last(), [0; 8]);
    }

    #[test]
    fn failed_button_down_is_not_carried_by_movement() {
        let reports = crate::test_support::Reports::default();
        let mut mouse = MouseState::new();
        reports.fail_on(&[1]);
        assert!(mouse.button_down(&reports, MouseButton::Left).is_err());
        mouse.move_relative(&reports, 1, 2).unwrap();
        assert_eq!(reports.last(), [0, 0, 1, 0, 2, 0, 0, 0]);
    }

    #[test]
    fn failed_button_up_keeps_button_held_until_retry() {
        let reports = crate::test_support::Reports::default();
        let mut mouse = MouseState::new();
        mouse.button_down(&reports, MouseButton::Left).unwrap();
        reports.fail_on(&[2]);
        assert!(mouse.button_up(&reports, MouseButton::Left).is_err());
        mouse.move_relative(&reports, 0, 0).unwrap();
        assert_eq!(reports.last()[0], 1);
        mouse.button_up(&reports, MouseButton::Left).unwrap();
        assert_eq!(reports.last()[0], 0);
    }

    #[test]
    fn clicking_held_button_does_not_release_it() {
        let reports = crate::test_support::Reports::default();
        let mut mouse = MouseState::new();
        mouse.button_down(&reports, MouseButton::Left).unwrap();
        mouse.click(&reports, MouseButton::Left).unwrap();
        mouse.move_relative(&reports, 0, 0).unwrap();
        assert_eq!(reports.last()[0], 1);
    }

    #[test]
    fn click_release_failure_cleans_new_button_but_keeps_old_button() {
        let reports = crate::test_support::Reports::default();
        let mut mouse = MouseState::new();
        mouse.button_down(&reports, MouseButton::Right).unwrap();
        reports.fail_on(&[3]);
        assert!(mouse.click(&reports, MouseButton::Left).is_err());
        mouse.move_relative(&reports, 0, 0).unwrap();
        assert_eq!(reports.last()[0], 2);
    }

    #[test]
    fn failed_drag_movement_releases_only_its_new_button() {
        for held in [MouseButton::Left, MouseButton::Right] {
            let reports = crate::test_support::Reports::default();
            let mut mouse = MouseState::new();
            mouse.button_down(&reports, held).unwrap();
            reports.fail_on(&[3]);
            let error = mouse
                .hold_button(&reports, MouseButton::Left, |mouse| {
                    mouse.move_relative(&reports, 100, 100)
                })
                .unwrap_err();
            assert!(matches!(
                error,
                crate::Error::Io { source, .. } if source.raw_os_error() == Some(3)
            ));
            mouse.move_relative(&reports, 0, 0).unwrap();
            assert_eq!(
                reports.last()[0],
                if held == MouseButton::Left { 1 } else { 2 }
            );
        }
    }

    #[test]
    fn relative_delta_chunks_preserve_total() {
        let mut remaining = 40_000;
        assert_eq!(next_chunk(&mut remaining), Some(32_767));
        assert_eq!(next_chunk(&mut remaining), Some(7_233));
        assert_eq!(next_chunk(&mut remaining), None);

        let mut remaining = -40_000;
        assert_eq!(next_chunk(&mut remaining), Some(-32_768));
        assert_eq!(next_chunk(&mut remaining), Some(-7_232));
        assert_eq!(next_chunk(&mut remaining), None);

        let mut remaining = i32::MIN;
        let mut total = 0i64;
        while let Some(step) = next_chunk(&mut remaining) {
            total += step as i64;
        }
        assert_eq!(total, i32::MIN as i64);
    }
}
