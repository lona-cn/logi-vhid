//! Driver-level keyboard and mouse input through Logitech G HUB's virtual HID
//! bus.
//!
//! The G HUB bus driver (`logi_joy_bus_enum`) exposes a control device with
//! the interface GUID `{DFBEDCDD-2148-416D-9E4D-CECC2424128C}`.  It creates
//! child HID devices (`VID_046D&PID_C232` keyboard, `PID_C231` mouse) and
//! accepts raw HID input reports through custom IOCTLs.
//!
//! The virtual mouse reports relative movement (16-bit X/Y), not absolute
//! coordinates. [`LogitechVirtualHid::set_cursor_pos_driver`] reads the cursor
//! position and sends relative movements until it reaches the target or the
//! attempt limit. It returns an error if it cannot reach the target; it does
//! not switch to another method. To position the cursor through Windows
//! `SendInput` instead, use [`AbsoluteMode::Win32`] or
//! [`LogitechVirtualHid::set_cursor_pos_win32`].
//!
//! This library uses the virtual HID devices provided by the Logitech G HUB
//! driver, which must be installed and running. It does not bypass code
//! signing, driver signature enforcement, or Windows security restrictions.
//!
//! # Requirements
//!
//! Windows 10 version 1703 or later, x64, with Logitech G HUB installed and
//! running. The minimum supported Rust version is 1.96.
//!
//! # Example
//!
//! This example is compiled but not run: running it sends keyboard input to the
//! active desktop application.
//!
//! ```no_run
//! use logi_vhid::{hid, KeyboardLayout, LogitechVirtualHid, OpenOptions};
//!
//! # fn main() -> logi_vhid::Result<()> {
//! let mut device = LogitechVirtualHid::open_with_options(OpenOptions::keyboard_only())?;
//! device.press(hid::F24)?;
//! device.type_text_layout("Hello", KeyboardLayout::Us)?;
//! # Ok(())
//! # }
//! ```

#![allow(non_snake_case)]
#![allow(clippy::upper_case_acronyms)]

pub mod capi;
mod cursor;
mod device;
mod diagnostics;
mod error;
mod ffi;
mod keyboard;
mod mouse;
pub mod protocol;
mod screen;
mod sequence;
#[cfg(test)]
mod test_support;

use std::thread::sleep;
use std::time::Duration;

pub use cursor::{cursor_pos, is_key_down, AbsoluteMode};
pub use diagnostics::DeviceInfo;
pub use error::{Error, Result};
pub use ffi::{Point, Rect};
pub use keyboard::{hid, vk_to_usage, KeyboardLayout, TextMode};
pub use mouse::MouseButton;
pub use protocol::{
    DEVICE_KIND_JOYSTICK, DEVICE_KIND_KEYBOARD, DEVICE_KIND_MOUSE, IOCTL_CREATE_DEVICE,
    IOCTL_KEYBOARD_REPORT, IOCTL_MOUSE_REPORT, IOCTL_QUERY_PIDS, IOCTL_REGISTER_PID,
    IOCTL_REMOVE_DEVICE,
};
pub use screen::{
    client_rect, client_to_screen, find_window_by_title, foreground_window, monitor_from_point,
    monitors, screen_to_client, virtual_desktop_rect, window_dpi, window_rect, MonitorInfo,
};
pub use sequence::{Action, Sequence};

use device::Device;
use keyboard::KeyboardState;
use mouse::MouseState;

const LOGITECH_VID: u16 = 0x046D;
const KEYBOARD_PID: u16 = 0xC232;
const MOUSE_PID: u16 = 0xC231;
const DEVICE_WAIT: Duration = Duration::from_millis(2_000);

/// Options for [`LogitechVirtualHid::open_with_options`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OpenOptions {
    /// Create and verify the virtual keyboard.
    pub keyboard: bool,
    /// Create and verify the virtual mouse.
    pub mouse: bool,
}

impl OpenOptions {
    pub const fn both() -> Self {
        Self {
            keyboard: true,
            mouse: true,
        }
    }

    pub const fn keyboard_only() -> Self {
        Self {
            keyboard: true,
            mouse: false,
        }
    }

    pub const fn mouse_only() -> Self {
        Self {
            keyboard: false,
            mouse: true,
        }
    }
}

impl Default for OpenOptions {
    fn default() -> Self {
        Self::both()
    }
}

/// A handle to the Logitech G HUB virtual HID bus.
pub struct LogitechVirtualHid {
    pub(crate) device: Device,
    pub(crate) keyboard: KeyboardState,
    pub(crate) mouse: MouseState,
    pub(crate) keyboard_enabled: bool,
    pub(crate) mouse_enabled: bool,
}

impl LogitechVirtualHid {
    /// Open the G HUB virtual bus and make sure the virtual keyboard and mouse
    /// exist.  Creating an existing device is treated as success.
    pub fn open() -> Result<Self> {
        Self::open_with_options(OpenOptions::both())
    }

    /// Open only the selected virtual devices and verify that they appeared in
    /// PnP after creation.  This avoids silently succeeding when the create
    /// request was rejected.
    pub fn open_with_options(options: OpenOptions) -> Result<Self> {
        let device = Device::open()?;
        if options.keyboard {
            device.create_device(DEVICE_KIND_KEYBOARD)?;
            device::wait_for_hid_device(LOGITECH_VID, KEYBOARD_PID, DEVICE_WAIT)?;
        }
        if options.mouse {
            device.create_device(DEVICE_KIND_MOUSE)?;
            device::wait_for_hid_device(LOGITECH_VID, MOUSE_PID, DEVICE_WAIT)?;
        }

        Ok(Self {
            device,
            keyboard: KeyboardState::new(),
            mouse: MouseState::new(),
            keyboard_enabled: options.keyboard,
            mouse_enabled: options.mouse,
        })
    }

    fn require_keyboard(&self) -> Result<()> {
        if self.keyboard_enabled {
            Ok(())
        } else {
            Err(Error::Unsupported(
                "keyboard was not opened; use OpenOptions with keyboard=true".to_string(),
            ))
        }
    }

    fn require_mouse(&self) -> Result<()> {
        if self.mouse_enabled {
            Ok(())
        } else {
            Err(Error::Unsupported(
                "mouse was not opened; use OpenOptions with mouse=true".to_string(),
            ))
        }
    }

    /// The device interface path that was opened.
    pub fn device_path(&self) -> &str {
        self.device.path()
    }

    /// Press a HID keyboard usage id (see [`hid`]).
    pub fn key_down(&mut self, usage: u8) -> Result<()> {
        self.require_keyboard()?;
        self.keyboard.key_down(&self.device, usage)
    }

    /// Release a HID keyboard usage id.
    pub fn key_up(&mut self, usage: u8) -> Result<()> {
        self.require_keyboard()?;
        self.keyboard.key_up(&self.device, usage)
    }

    /// Press and release a HID usage id.
    pub fn press(&mut self, usage: u8) -> Result<()> {
        self.require_keyboard()?;
        self.keyboard.press(&self.device, usage)
    }

    /// Press and release a Win32 virtual-key code.
    pub fn press_vk(&mut self, vk: u16) -> Result<()> {
        self.require_keyboard()?;
        let usage = vk_to_usage(vk).ok_or_else(|| {
            Error::Unsupported(format!("no HID usage mapping for virtual-key 0x{vk:04X}"))
        })?;
        self.press(usage)
    }

    /// Press a key combination in order, hold it briefly, then release in reverse
    /// order. Put modifier keys such as Ctrl or Shift first.
    pub fn press_combo(&mut self, usages: &[u8]) -> Result<()> {
        self.require_keyboard()?;
        self.keyboard.press_combo(&self.device, usages)
    }

    /// Press a key combination expressed as Win32 virtual-key codes.
    pub fn press_vk_combo(&mut self, vks: &[u16]) -> Result<()> {
        self.require_keyboard()?;
        if vks.len() > 14 {
            return Err(Error::Unsupported(
                "keyboard combo has more than 14 virtual keys".to_string(),
            ));
        }
        let mut usages = [0u8; 14];
        for (index, &vk) in vks.iter().enumerate() {
            usages[index] = vk_to_usage(vk).ok_or_else(|| {
                Error::Unsupported(format!("no HID usage mapping for virtual-key 0x{vk:04X}"))
            })?;
        }
        self.press_combo(&usages[..vks.len()])
    }

    /// Hold a HID usage for a duration, then release it.
    pub fn hold_key(&mut self, usage: u8, duration: Duration) -> Result<()> {
        self.require_keyboard()?;
        self.keyboard.hold_key(&self.device, usage, duration)
    }

    /// Repeat a HID usage `count` times with `interval` between presses.
    pub fn repeat_key(&mut self, usage: u8, count: u32, interval: Duration) -> Result<()> {
        self.require_keyboard()?;
        self.keyboard
            .repeat_key(&self.device, usage, count, interval)
    }

    /// Type printable ASCII through the virtual keyboard.  Letters, digits,
    /// punctuation, space, tab and newline are supported; other characters
    /// return [`Error::Unsupported`].
    pub fn type_ascii(&mut self, text: &str) -> Result<()> {
        self.require_keyboard()?;
        self.keyboard.type_ascii(&self.device, text)
    }

    /// Type text through the HID keyboard using a Windows keyboard layout.
    pub fn type_text_layout(&mut self, text: &str, layout: KeyboardLayout) -> Result<()> {
        self.require_keyboard()?;
        self.keyboard.type_text_layout(&self.device, text, layout)
    }

    /// Type arbitrary Unicode with Win32 `SendInput` / `KEYEVENTF_UNICODE`.
    /// This does not use the Logitech driver.
    pub fn type_unicode(&mut self, text: &str) -> Result<()> {
        keyboard::type_unicode(text)
    }

    /// Type text with the selected mode.
    pub fn type_text(&mut self, text: &str, mode: TextMode) -> Result<()> {
        match mode {
            TextMode::Hid(layout) => self.type_text_layout(text, layout),
            TextMode::Win32Unicode => self.type_unicode(text),
        }
    }

    /// Release every key currently held by this object.
    pub fn release_all(&mut self) -> Result<()> {
        self.require_keyboard()?;
        self.keyboard.release_all(&self.device)
    }

    /// Move the virtual mouse by a relative delta.  Large deltas are split
    /// into 16-bit reports instead of being silently clamped.
    pub fn mouse_move_relative(&mut self, dx: i32, dy: i32) -> Result<()> {
        self.require_mouse()?;
        self.mouse.move_relative(&self.device, dx, dy)
    }

    /// Hold a mouse button.
    pub fn mouse_button_down(&mut self, button: MouseButton) -> Result<()> {
        self.require_mouse()?;
        self.mouse.button_down(&self.device, button)
    }

    /// Release a mouse button.
    pub fn mouse_button_up(&mut self, button: MouseButton) -> Result<()> {
        self.require_mouse()?;
        self.mouse.button_up(&self.device, button)
    }

    /// Click a mouse button.
    pub fn mouse_click(&mut self, button: MouseButton) -> Result<()> {
        self.require_mouse()?;
        self.mouse.click(&self.device, button)
    }

    /// Double-click a mouse button.
    pub fn mouse_double_click(&mut self, button: MouseButton) -> Result<()> {
        self.require_mouse()?;
        self.mouse.click(&self.device, button)?;
        sleep(Duration::from_millis(40));
        self.mouse.click(&self.device, button)
    }

    /// Turn the wheel by `delta` (positive = away from the user).
    pub fn mouse_wheel(&mut self, delta: i8) -> Result<()> {
        self.require_mouse()?;
        self.mouse.wheel(&self.device, delta)
    }

    /// Move to `(x, y)` and click a button.
    pub fn click_at(
        &mut self,
        x: i32,
        y: i32,
        button: MouseButton,
        mode: AbsoluteMode,
    ) -> Result<()> {
        self.require_mouse()?;
        self.set_cursor_pos(x, y, mode)?;
        self.mouse_click(button)
    }

    /// Move to `(x, y)` and double-click a button.
    pub fn double_click_at(
        &mut self,
        x: i32,
        y: i32,
        button: MouseButton,
        mode: AbsoluteMode,
    ) -> Result<()> {
        self.require_mouse()?;
        self.set_cursor_pos(x, y, mode)?;
        self.mouse_double_click(button)
    }

    /// Press at `from`, move to `to`, and release inputs introduced by this
    /// operation, including on failure. Previously held buttons stay held.
    pub fn drag(
        &mut self,
        from: (i32, i32),
        to: (i32, i32),
        button: MouseButton,
        mode: AbsoluteMode,
    ) -> Result<()> {
        self.require_mouse()?;
        self.set_cursor_pos(from.0, from.1, mode)?;
        let bus = &self.device;
        self.mouse.hold_button(bus, button, |mouse| {
            match mode {
                AbsoluteMode::Driver => cursor::set_cursor_pos_driver(bus, mouse, to.0, to.1)?,
                AbsoluteMode::Win32 => cursor::set_cursor_pos_win32(to.0, to.1)?,
                AbsoluteMode::SetCursorPos => cursor::set_cursor_pos_direct(to.0, to.1)?,
            }
            sleep(Duration::from_millis(20));
            Ok(())
        })
    }

    /// Move to a client-relative point in a window and click.
    pub fn click_window_relative(
        &mut self,
        window: isize,
        x: i32,
        y: i32,
        button: MouseButton,
        mode: AbsoluteMode,
    ) -> Result<()> {
        self.require_mouse()?;
        let point = screen::client_to_screen(window, Point { x, y })?;
        self.click_at(point.x, point.y, button, mode)
    }

    /// Read the absolute cursor position in physical pixels.
    pub fn cursor_pos() -> Result<Point> {
        cursor_pos()
    }

    /// Query whether a Win32 virtual key is currently down.
    pub fn is_key_down(vk: i32) -> bool {
        is_key_down(vk)
    }

    /// PIDs currently registered with IOCTL `0x2A2044`.
    ///
    /// A non-empty list means the driver zeroes mouse X/Y; this is the first
    /// thing to check when the virtual mouse stops moving.
    pub fn registered_pids(&self) -> Result<Vec<u64>> {
        self.device.registered_pids()
    }

    /// Remove one virtual device (`DEVICE_KIND_KEYBOARD` or `DEVICE_KIND_MOUSE`).
    /// A missing device is treated as success.
    pub fn remove_device(&self, kind: u32) -> Result<()> {
        self.device.remove_device(kind)
    }

    /// Set the absolute cursor position using the virtual HID mouse.
    ///
    /// The HID mouse is relative-only, so this reads the current cursor
    /// position and sends bounded relative reports (at most 64 counts each)
    /// until the target is reached.  It returns an error instead of falling
    /// back to Win32.
    pub fn set_cursor_pos_driver(&mut self, x: i32, y: i32) -> Result<()> {
        self.require_mouse()?;
        cursor::set_cursor_pos_driver(&self.device, &mut self.mouse, x, y)
    }

    /// Set the absolute cursor position with Win32 `SendInput` using
    /// `MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK`.
    ///
    /// This is an explicit Win32 path, not an automatic fallback.
    pub fn set_cursor_pos_win32(x: i32, y: i32) -> Result<()> {
        cursor::set_cursor_pos_win32(x, y)
    }

    /// Set the absolute cursor position with Win32 `SetCursorPos`.
    ///
    /// This moves the cursor directly rather than using `SendInput`.
    pub fn set_cursor_pos_direct(x: i32, y: i32) -> Result<()> {
        cursor::set_cursor_pos_direct(x, y)
    }

    /// Set the absolute cursor position with the selected mode.
    pub fn set_cursor_pos(&mut self, x: i32, y: i32, mode: AbsoluteMode) -> Result<()> {
        match mode {
            AbsoluteMode::Driver => self.set_cursor_pos_driver(x, y),
            AbsoluteMode::Win32 => Self::set_cursor_pos_win32(x, y),
            AbsoluteMode::SetCursorPos => Self::set_cursor_pos_direct(x, y),
        }
    }
}
