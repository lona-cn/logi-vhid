//! Sequential input actions.
//!
//! Actions run on the calling thread, in order. The library does not start a
//! background thread or provide cancellation.

use std::time::Duration;

use crate::device::ReportSink;
use crate::error::{Error, Result};
use crate::keyboard::{self, KeyboardState};
use crate::mouse::MouseState;
use crate::{AbsoluteMode, LogitechVirtualHid, MouseButton, TextMode};

/// One action in a [`Sequence`].
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

/// An ordered list of input actions.
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

    /// Run actions in order. On failure, attempt to release keys and buttons
    /// left pressed by this run. Keep those still held from before the run.
    /// If releasing them also fails, return the original action error.
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

    fn run_inputs(
        &self,
        bus: &impl ReportSink,
        keyboard: &mut KeyboardState,
        mouse: &mut MouseState,
        enabled: (bool, bool),
        mut absolute: impl FnMut(&mut MouseState, i32, i32, AbsoluteMode) -> Result<()>,
    ) -> Result<()> {
        let mut initial_keyboard = *keyboard;
        let mut initial_mouse = *mouse;
        for action in &self.actions {
            let result = (|| {
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
                    Action::Sleep(duration) => std::thread::sleep(*duration),
                    Action::MoveRelative { dx, dy } => mouse.move_relative(bus, *dx, *dy)?,
                    Action::MoveAbsolute { x, y, mode } => absolute(mouse, *x, *y, *mode)?,
                    Action::MouseDown(button) => mouse.button_down(bus, *button)?,
                    Action::MouseUp(button) => mouse.button_up(bus, *button)?,
                    Action::Click(button) => mouse.click(bus, *button)?,
                    Action::DoubleClick(button) => {
                        mouse.click(bus, *button)?;
                        std::thread::sleep(Duration::from_millis(40));
                        mouse.click(bus, *button)?;
                    }
                    Action::Wheel(delta) => mouse.wheel(bus, *delta)?,
                    Action::KeyDown(usage) => keyboard.key_down(bus, *usage)?,
                    Action::KeyUp(usage) => keyboard.key_up(bus, *usage)?,
                    Action::Press(usage) => keyboard.press(bus, *usage)?,
                    Action::PressCombo(usages) => keyboard.press_combo(bus, usages)?,
                    Action::Hold { usage, duration } => {
                        keyboard.hold_key(bus, *usage, *duration)?
                    }
                    Action::Repeat {
                        usage,
                        count,
                        interval,
                    } => keyboard.repeat_key(bus, *usage, *count, *interval)?,
                    Action::TypeAscii(text) => keyboard.type_ascii(bus, text)?,
                    Action::TypeText {
                        text,
                        mode: TextMode::Hid(layout),
                    } => keyboard.type_text_layout(bus, text, *layout)?,
                    Action::TypeText {
                        text,
                        mode: TextMode::Win32Unicode,
                    } => keyboard::type_unicode(text)?,
                }
                Ok(())
            })();
            if let Err(error) = result {
                let _ = keyboard.cleanup_since(bus, &initial_keyboard);
                let _ = mouse.cleanup_since(bus, &initial_mouse);
                return Err(error);
            }
            // An explicitly released old input belongs to the sequence if
            // pressed again later; it must no longer be protected by the baseline.
            initial_keyboard.retain_held(keyboard);
            initial_mouse.retain_held(mouse);
        }
        Ok(())
    }

    /// Run the sequence repeatedly with a delay between repetitions.
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
}
