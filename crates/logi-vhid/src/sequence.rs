//! Sequential input actions.
//!
//! Actions run on the calling thread, in order. Cooperative cancellation is
//! available through [`Sequence::run_controlled`]; no background thread is used.

use std::time::Duration;

use crate::device::ReportSink;
use crate::error::{Error, Result};
use crate::execution::{ControlledSink, ExecutionContext, ExecutionFailure, ReleaseReport};
use crate::keyboard::{self, KeyboardState};
use crate::mouse::MouseState;
use crate::{AbsoluteMode, LogitechVirtualHid, MouseButton, TextMode};

/// One action in a [`Sequence`].
///
/// Key values are raw HID usages, not virtual-key codes. Relative movement uses
/// HID counts; absolute movement uses physical screen pixels; wheel values use
/// HID wheel units, not Win32 multiples of 120. Text modes retain the layout and
/// partial-input limitations of [`LogitechVirtualHid::type_text`].
#[derive(Clone, Debug)]
pub enum Action {
    Sleep(Duration),
    MoveRelative {
        dx: i32,
        dy: i32,
    },
    MoveAbsolute {
        x: i32,
        y: i32,
        mode: AbsoluteMode,
    },
    MouseDown(MouseButton),
    MouseUp(MouseButton),
    Click(MouseButton),
    DoubleClick(MouseButton),
    Wheel(i8),
    KeyDown(u8),
    KeyUp(u8),
    Press(u8),
    PressCombo(Vec<u8>),
    Hold {
        usage: u8,
        duration: Duration,
    },
    Repeat {
        usage: u8,
        count: u32,
        interval: Duration,
    },
    TypeAscii(String),
    TypeText {
        text: String,
        mode: TextMode,
    },
}

/// An ordered list of input actions, built without sending input.
///
/// Execution is synchronous on the calling thread, including sleeps. Successful
/// execution leaves explicit `KeyDown`/`MouseDown` actions held until a later
/// release; dropping the sequence or device does not automatically release them.
#[derive(Clone, Debug, Default)]
pub struct Sequence {
    pub actions: Vec<Action>,
}

impl Sequence {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(mut self, action: Action) -> Self {
        self.actions.push(action);
        self
    }

    pub fn sleep(mut self, duration: Duration) -> Self {
        self.actions.push(Action::Sleep(duration));
        self
    }

    pub fn move_relative(mut self, dx: i32, dy: i32) -> Self {
        self.actions.push(Action::MoveRelative { dx, dy });
        self
    }

    pub fn move_absolute(mut self, x: i32, y: i32, mode: AbsoluteMode) -> Self {
        self.actions.push(Action::MoveAbsolute { x, y, mode });
        self
    }

    pub fn click(mut self, button: MouseButton) -> Self {
        self.actions.push(Action::Click(button));
        self
    }

    pub fn press(mut self, usage: u8) -> Self {
        self.actions.push(Action::Press(usage));
        self
    }

    pub fn type_ascii(mut self, text: impl Into<String>) -> Self {
        self.actions.push(Action::TypeAscii(text.into()));
        self
    }

    /// Run actions in order, stopping at the first failure.
    ///
    /// There is no up-front validation or rollback: completed input and cursor
    /// movement remain. On failure, attempt to release keys/buttons introduced
    /// by this run, preserving old inputs only while they remain continuously
    /// held. Explicitly released old inputs are not pressed again.
    /// Cleanup may fail too; the original action error is returned regardless.
    ///
    /// HID actions require the matching device option. Win32 absolute movement
    /// and Unicode text do not require mouse/keyboard flags, respectively.
    /// On success, inputs explicitly left held are **not** cleaned up.
    pub fn run(&self, device: &mut LogitechVirtualHid) -> Result<()> {
        let bus = &device.device;
        self.run_inputs(
            bus,
            &mut device.keyboard,
            &mut device.mouse,
            (device.keyboard_enabled, device.mouse_enabled),
            |mouse, x, y, mode| match mode {
                AbsoluteMode::Driver => crate::cursor::set_cursor_pos_driver(bus, mouse, x, y),
                AbsoluteMode::Win32 => crate::cursor::set_cursor_pos_win32(x, y),
                AbsoluteMode::SetCursorPos => crate::cursor::set_cursor_pos_direct(x, y),
            },
        )
    }

    /// Run with a caller-provided cooperative checkpoint at safe boundaries.
    ///
    /// Waits check at most every 10 ms. Synchronous Windows/driver calls cannot
    /// be forcibly interrupted. On failure cleanup bypasses the checkpoint and
    /// attempts both selected categories, preserving continuously held baseline
    /// inputs. Neither execution nor cleanup rolls back already submitted input.
    pub fn run_controlled(
        &self,
        device: &mut LogitechVirtualHid,
        checkpoint: &dyn Fn() -> Result<()>,
    ) -> std::result::Result<(), ExecutionFailure> {
        let control = ExecutionContext::controlled(checkpoint);
        let bus = &device.device;
        let sink = ControlledSink { sink: bus, control };
        self.run_inputs_controlled(
            bus,
            &mut device.keyboard,
            &mut device.mouse,
            (device.keyboard_enabled, device.mouse_enabled),
            control,
            |mouse, x, y, mode| match mode {
                AbsoluteMode::Driver => crate::cursor::set_cursor_pos_driver_with_control(
                    bus, &sink, mouse, x, y, control,
                ),
                AbsoluteMode::Win32 => crate::cursor::set_cursor_pos_win32(x, y),
                AbsoluteMode::SetCursorPos => crate::cursor::set_cursor_pos_direct(x, y),
            },
        )
    }

    fn run_inputs(
        &self,
        bus: &impl ReportSink,
        keyboard: &mut KeyboardState,
        mouse: &mut MouseState,
        enabled: (bool, bool),
        absolute: impl FnMut(&mut MouseState, i32, i32, AbsoluteMode) -> Result<()>,
    ) -> Result<()> {
        self.run_inputs_controlled(
            bus,
            keyboard,
            mouse,
            enabled,
            ExecutionContext::uncontrolled(),
            absolute,
        )
        .map_err(|failure| failure.cause)
    }

    fn run_inputs_controlled(
        &self,
        bus: &impl ReportSink,
        keyboard: &mut KeyboardState,
        mouse: &mut MouseState,
        enabled: (bool, bool),
        control: ExecutionContext<'_>,
        mut absolute: impl FnMut(&mut MouseState, i32, i32, AbsoluteMode) -> Result<()>,
    ) -> std::result::Result<(), ExecutionFailure> {
        let sink = ControlledSink { sink: bus, control };
        let bus = &sink;
        let mut initial_keyboard = *keyboard;
        let mut initial_mouse = *mouse;
        for action in &self.actions {
            let result = (|| {
                control.check()?;
                match action {
                    Action::KeyDown(_)
                    | Action::KeyUp(_)
                    | Action::Press(_)
                    | Action::PressCombo(_)
                    | Action::Hold { .. }
                    | Action::Repeat { .. }
                    | Action::TypeAscii(_)
                    | Action::TypeText {
                        mode: TextMode::Hid(_),
                        ..
                    } if !enabled.0 => {
                        return Err(Error::Unsupported(
                            "keyboard was not opened; use OpenOptions with keyboard=true"
                                .to_string(),
                        ));
                    }
                    Action::MoveRelative { .. }
                    | Action::MouseDown(_)
                    | Action::MouseUp(_)
                    | Action::Click(_)
                    | Action::DoubleClick(_)
                    | Action::Wheel(_)
                    | Action::MoveAbsolute {
                        mode: AbsoluteMode::Driver,
                        ..
                    } if !enabled.1 => {
                        return Err(Error::Unsupported(
                            "mouse was not opened; use OpenOptions with mouse=true".to_string(),
                        ));
                    }
                    _ => {}
                }
                match action {
                    Action::Sleep(duration) => control.wait(*duration)?,
                    Action::MoveRelative { dx, dy } => {
                        mouse.move_relative_with_control(bus, *dx, *dy, control)?
                    }
                    Action::MoveAbsolute { x, y, mode } => absolute(mouse, *x, *y, *mode)?,
                    Action::MouseDown(button) => mouse.button_down(bus, *button)?,
                    Action::MouseUp(button) => mouse.button_up(bus, *button)?,
                    Action::Click(button) => mouse.click_with_control(bus, *button, control)?,
                    Action::DoubleClick(button) => {
                        mouse.click_with_control(bus, *button, control)?;
                        control.wait(Duration::from_millis(40))?;
                        mouse.click_with_control(bus, *button, control)?;
                    }
                    Action::Wheel(delta) => mouse.wheel(bus, *delta)?,
                    Action::KeyDown(usage) => keyboard.key_down(bus, *usage)?,
                    Action::KeyUp(usage) => keyboard.key_up(bus, *usage)?,
                    Action::Press(usage) => keyboard.press_with_control(bus, *usage, control)?,
                    Action::PressCombo(usages) => {
                        keyboard.press_combo_with_control(bus, usages, control)?
                    }
                    Action::Hold { usage, duration } => {
                        keyboard.hold_key_with_control(bus, *usage, *duration, control)?
                    }
                    Action::Repeat {
                        usage,
                        count,
                        interval,
                    } => {
                        keyboard.repeat_key_with_control(bus, *usage, *count, *interval, control)?
                    }
                    Action::TypeAscii(text) => {
                        keyboard.type_ascii_with_control(bus, text, control)?
                    }
                    Action::TypeText {
                        text,
                        mode: TextMode::Hid(layout),
                    } => keyboard.type_text_layout_with_control(bus, text, *layout, control)?,
                    Action::TypeText {
                        text,
                        mode: TextMode::Win32Unicode,
                    } => keyboard::type_unicode_with_control(text, control)?,
                }
                Ok(())
            })();
            if let Err(cause) = result {
                return Err(ExecutionFailure {
                    cause,
                    cleanup: ReleaseReport {
                        keyboard: enabled
                            .0
                            .then(|| keyboard.cleanup_since(bus, &initial_keyboard)),
                        mouse: enabled.1.then(|| mouse.cleanup_since(bus, &initial_mouse)),
                    },
                });
            }
            // An explicitly released old input belongs to the sequence if
            // pressed again later; it must no longer be protected by the baseline.
            initial_keyboard.retain_held(keyboard);
            initial_mouse.retain_held(mouse);
        }
        control.check().map_err(|cause| ExecutionFailure {
            cause,
            cleanup: ReleaseReport {
                keyboard: enabled
                    .0
                    .then(|| keyboard.cleanup_since(bus, &initial_keyboard)),
                mouse: enabled.1.then(|| mouse.cleanup_since(bus, &initial_mouse)),
            },
        })
    }

    /// Run synchronously `times` times with `interval` after each completed run
    /// except the last. Zero repetitions do nothing. Stops on the first failure.
    /// Held inputs persist between successful repetitions, so each run's cleanup
    /// baseline is the state at the start of that repetition.
    pub fn run_repeat(
        &self,
        device: &mut LogitechVirtualHid,
        times: u32,
        interval: Duration,
    ) -> Result<()> {
        for index in 0..times {
            self.run(device)?;
            if index + 1 < times {
                std::thread::sleep(interval);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keyboard::{hid, KeyboardState};
    use crate::mouse::MouseState;
    use crate::test_support::Reports;

    #[test]
    fn failure_cleans_only_new_inputs_and_stops_following_actions() {
        let reports = Reports::default();
        let mut keyboard = KeyboardState::new();
        let mut mouse = MouseState::new();
        keyboard.key_down(&reports, hid::B).unwrap();
        keyboard.key_down(&reports, hid::C).unwrap();
        mouse.button_down(&reports, MouseButton::Right).unwrap();
        mouse.button_down(&reports, MouseButton::Middle).unwrap();
        let sequence = Sequence::new()
            .push(Action::KeyUp(hid::C))
            .push(Action::MouseUp(MouseButton::Middle))
            .push(Action::KeyDown(hid::C))
            .push(Action::MouseDown(MouseButton::Middle))
            .push(Action::KeyDown(hid::A))
            .push(Action::MouseDown(MouseButton::Left))
            .push(Action::KeyDown(0))
            .push(Action::KeyDown(hid::D));
        let result = sequence.run_inputs(
            &reports,
            &mut keyboard,
            &mut mouse,
            (true, true),
            |_, _, _, _| panic!("no absolute actions"),
        );
        assert!(result.is_err());
        keyboard.key_down(&reports, hid::E).unwrap();
        assert_eq!(reports.last(), [0, 0, hid::B, hid::E, 0, 0, 0, 0]);
        mouse.move_relative(&reports, 0, 0).unwrap();
        assert_eq!(reports.last()[0], 2);
        assert!(reports.accepted().iter().all(|(code, report)| *code
            != crate::protocol::IOCTL_KEYBOARD_REPORT
            || !report[2..].contains(&hid::D)));
    }

    #[test]
    fn cleanup_failure_still_cleans_other_input_kind_and_preserves_original_error() {
        use crate::protocol::{IOCTL_KEYBOARD_REPORT, IOCTL_MOUSE_REPORT};

        for rejected_kind in [IOCTL_KEYBOARD_REPORT, IOCTL_MOUSE_REPORT] {
            let reports = Reports::default();
            let mut keyboard = KeyboardState::new();
            let mut mouse = MouseState::new();
            reports.fail_on(&[4]);
            reports.reject_report(rejected_kind, [0; 8]);
            let sequence = Sequence::new()
                .push(Action::KeyDown(hid::A))
                .push(Action::MouseDown(MouseButton::Left))
                .push(Action::KeyDown(hid::B))
                .push(Action::MouseDown(MouseButton::Right));
            let error = sequence
                .run_inputs(
                    &reports,
                    &mut keyboard,
                    &mut mouse,
                    (true, true),
                    |_, _, _, _| panic!("no absolute actions"),
                )
                .unwrap_err();
            assert!(matches!(
                error,
                Error::Io { source, .. } if source.raw_os_error() == Some(4)
            ));
            keyboard.key_down(&reports, hid::C).unwrap();
            assert_eq!(
                reports.last(),
                if rejected_kind == IOCTL_KEYBOARD_REPORT {
                    [0, 0, hid::A, hid::B, hid::C, 0, 0, 0]
                } else {
                    [0, 0, hid::C, 0, 0, 0, 0, 0]
                }
            );
            mouse.move_relative(&reports, 0, 0).unwrap();
            assert_eq!(
                reports.last()[0],
                u8::from(rejected_kind == IOCTL_MOUSE_REPORT)
            );
        }
    }

    struct CancelAfter<'a> {
        reports: &'a Reports,
        cancelled: &'a std::cell::Cell<bool>,
        after: usize,
    }

    impl ReportSink for CancelAfter<'_> {
        fn send_report(&self, code: u32, report: &[u8; 8]) -> Result<()> {
            self.reports.send_report(code, report)?;
            if self.reports.count() == self.after {
                self.cancelled.set(true);
            }
            Ok(())
        }
    }

    #[test]
    fn controlled_cancel_releases_new_inputs_preserves_baseline_and_stops_business_reports() {
        use crate::execution::ExecutionContext;
        use crate::protocol::{IOCTL_KEYBOARD_REPORT, IOCTL_MOUSE_REPORT};
        let reports = Reports::default();
        let mut keyboard = KeyboardState::new();
        let mut mouse = MouseState::new();
        keyboard.key_down(&reports, hid::B).unwrap();
        mouse.button_down(&reports, MouseButton::Right).unwrap();
        let cancelled = std::cell::Cell::new(false);
        let sink = CancelAfter {
            reports: &reports,
            cancelled: &cancelled,
            after: 4,
        };
        let checkpoint = || {
            if cancelled.get() {
                Err(Error::Cancelled)
            } else {
                Ok(())
            }
        };
        let failure = Sequence::new()
            .push(Action::KeyDown(hid::A))
            .push(Action::MouseDown(MouseButton::Left))
            .push(Action::KeyDown(hid::D))
            .run_inputs_controlled(
                &sink,
                &mut keyboard,
                &mut mouse,
                (true, true),
                ExecutionContext::controlled(&checkpoint),
                |_, _, _, _| panic!("no absolute movement"),
            )
            .unwrap_err();
        assert!(matches!(failure.cause, Error::Cancelled));
        assert!(matches!(failure.cleanup.keyboard, Some(Ok(()))));
        assert!(matches!(failure.cleanup.mouse, Some(Ok(()))));
        assert_eq!(
            reports.accepted(),
            vec![
                (IOCTL_KEYBOARD_REPORT, [0, 0, hid::B, 0, 0, 0, 0, 0]),
                (IOCTL_MOUSE_REPORT, [2, 0, 0, 0, 0, 0, 0, 0]),
                (IOCTL_KEYBOARD_REPORT, [0, 0, hid::B, hid::A, 0, 0, 0, 0]),
                (IOCTL_MOUSE_REPORT, [3, 0, 0, 0, 0, 0, 0, 0]),
                (IOCTL_KEYBOARD_REPORT, [0, 0, hid::B, 0, 0, 0, 0, 0]),
                (IOCTL_MOUSE_REPORT, [2, 0, 0, 0, 0, 0, 0, 0]),
            ]
        );
    }

    #[test]
    fn controlled_failure_exposes_both_cleanup_errors_and_keeps_cause() {
        use crate::execution::ExecutionContext;
        let reports = Reports::default();
        let mut keyboard = KeyboardState::new();
        let mut mouse = MouseState::new();
        reports.fail_on(&[3, 4, 5]);
        let failure = Sequence::new()
            .push(Action::KeyDown(hid::A))
            .push(Action::MouseDown(MouseButton::Left))
            .push(Action::KeyDown(hid::B))
            .run_inputs_controlled(
                &reports,
                &mut keyboard,
                &mut mouse,
                (true, true),
                ExecutionContext::controlled(&|| Ok(())),
                |_, _, _, _| panic!("no absolute movement"),
            )
            .unwrap_err();
        assert!(
            matches!(failure.cause, Error::Io { ref source, .. } if source.raw_os_error() == Some(3))
        );
        assert!(
            matches!(failure.cleanup.keyboard, Some(Err(Error::Io { ref source, .. })) if source.raw_os_error() == Some(4))
        );
        assert!(
            matches!(failure.cleanup.mouse, Some(Err(Error::Io { ref source, .. })) if source.raw_os_error() == Some(5))
        );
        let display = failure.to_string();
        assert!(display.contains("keyboard") && display.contains("mouse"));
        assert_eq!(reports.count(), 5);
    }

    #[test]
    fn controlled_deadline_before_first_action_submits_no_input() {
        use crate::execution::ExecutionContext;
        let reports = Reports::default();
        let failure = Sequence::new()
            .press(hid::A)
            .run_inputs_controlled(
                &reports,
                &mut KeyboardState::new(),
                &mut MouseState::new(),
                (true, true),
                ExecutionContext::controlled(&|| Err(Error::TimedOut)),
                |_, _, _, _| panic!("no absolute movement"),
            )
            .unwrap_err();
        assert!(matches!(failure.cause, Error::TimedOut));
        assert_eq!(reports.count(), 0);
    }
}
