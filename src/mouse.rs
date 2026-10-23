//! Mouse report state and relative movement.

use std::thread::sleep;
use std::time::Duration;

use crate::device::ReportSink;
use crate::error::Result;
use crate::protocol::{MouseReport, IOCTL_MOUSE_REPORT};

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
        if dx == 0 && dy == 0 {
            return self.send(device, 0, 0, 0, 0);
        }

        let mut remaining_x = dx;
        let mut remaining_y = dy;
        let mut first = true;
        loop {
            let step_x = next_chunk(&mut remaining_x);
            let step_y = next_chunk(&mut remaining_y);
            if step_x.is_none() && step_y.is_none() {
                break;
            }
            if !first {
                sleep(Duration::from_millis(1));
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
            next.send(device, 0, 0, 0, 0)?;
            *self = next;
        }
        Ok(())
    }

    pub(crate) fn click(&mut self, device: &impl ReportSink, button: MouseButton) -> Result<()> {
        self.hold_button(device, button, |_| Ok(()))
    }

    pub(crate) fn hold_button(
        &mut self,
        device: &impl ReportSink,
        button: MouseButton,
        action: impl FnOnce(&mut Self) -> Result<()>,
    ) -> Result<()> {
        let initial = *self;
        let result = (|| {
            self.button_down(device, button)?;
            sleep(Duration::from_millis(20));
            action(self)?;
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
