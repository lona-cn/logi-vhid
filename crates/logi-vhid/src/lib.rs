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
//! Opening a control handle requires the G HUB virtual bus driver to be installed
//! and available. The library does not bypass code signing, driver signature
//! enforcement, or Windows security restrictions.
//!
//! Input is sent to the active desktop, not addressed to a window or process.
//! HID usages are physical keys interpreted by the target's layout; relative
//! mouse values are HID counts affected by Windows pointer settings. Absolute
//! cursor and screen/window helpers use physical pixels, temporarily changing
//! only the calling thread's DPI context and attempting to restore it afterward.
//!
//! Successful report submission does not prove an application accepted input.
//! Win32 paths remain subject to Windows desktop, integrity-level/UIPI and target
//! application restrictions. Failures can follow partial input; cleanup is
//! best-effort. See [`LogitechVirtualHid`] for handle and held-input ownership.
//!
//! # Requirements
//!
//! Windows 10 version 1703 or later, x64. Opening a device requires an available
//! Logitech G HUB virtual bus driver. The minimum supported Rust version is 1.96.
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
mod execution;
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
pub use execution::{ExecutionFailure, ReleaseReport};
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
///
/// The default selects both devices. These flags gate HID operations on the
/// returned object; they do not select a physical Logitech peripheral or isolate
/// this object's reports from other clients of the same virtual devices.
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

/// An open control-device handle and local keyboard/mouse report state.
///
/// Input calls are synchronous and can block for their documented waits. Reports
/// target the shared virtual devices, not a particular window; the focused
/// application normally receives keyboard input. Coordinate and Unicode Win32
/// methods use Windows directly rather than the Logitech report path.
///
/// # Resource ownership
///
/// Dropping this object closes its control handle only. It does **not** send
/// key/button release reports or remove virtual devices. Explicitly release held
/// inputs before dropping it; [`Self::release_all`] releases keyboard keys only.
/// Multiple objects have independent local state but are not independent virtual
/// keyboards/mice. Avoid interleaving their reports or removing devices another
/// client is using.
///
/// Temporary operations (press, combo, text, click and drag) preserve inputs
/// already held by this object and attempt to release inputs they introduced,
/// even on failure. Cleanup can also fail; an error does not guarantee that all
/// inputs are released. No operation rolls back input already delivered.
pub struct LogitechVirtualHid {
    pub(crate) device: Device,
    pub(crate) keyboard: KeyboardState,
    pub(crate) mouse: MouseState,
    pub(crate) keyboard_enabled: bool,
    pub(crate) mouse_enabled: bool,
}

impl LogitechVirtualHid {
    /// Open the first enumerated G HUB bus control interface, creating and
    /// verifying both virtual devices. Equivalent to
    /// [`Self::open_with_options`] with [`OpenOptions::both`].
    pub fn open() -> Result<Self> {
        Self::open_with_options(OpenOptions::both())
    }

    /// Open the first enumerated G HUB bus control interface and create only the
    /// selected virtual devices.
    ///
    /// Each selected child is checked for up to two seconds by finding a present
    /// HID interface with the expected VID/PID. This is a presence check, not a
    /// report-delivery test or a check that the child belongs to this control
    /// interface. An existing child is accepted.
    ///
    /// Unselected HID operations return [`Error::Unsupported`]. Both flags may
    /// be false to open the control bus without creating children; the bus driver
    /// is still required. Win32 Unicode and Win32 cursor positioning do not
    /// require the corresponding flag.
    ///
    /// On failure the control handle is closed, but children already created
    /// are not removed. Successful objects also leave children in place on drop.
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

    /// Hold a raw HID Keyboard/Keypad usage ID (see [`hid`]), not a Win32
    /// virtual-key code or character.
    ///
    /// Up to six distinct non-modifier usages plus eight modifiers (`0xE0` through
    /// `0xE7`) may be held. Usage zero and a seventh non-modifier return
    /// [`Error::Unsupported`]. An already held usage is not duplicated.
    /// State changes are recorded only after the report succeeds.
    pub fn key_down(&mut self, usage: u8) -> Result<()> {
        self.require_keyboard()?;
        self.keyboard.key_down(&self.device, usage)
    }

    /// Release a raw HID usage, updating local state only after the report
    /// succeeds. A failed release leaves it held locally so it can be retried.
    /// Usage zero is a no-op; physical keyboard state is not changed.
    pub fn key_up(&mut self, usage: u8) -> Result<()> {
        self.require_keyboard()?;
        self.keyboard.key_up(&self.device, usage)
    }

    /// Press a HID usage for 20 ms, then release it unless it was already held
    /// by this object. An already held usage does not generate a new key-down edge.
    pub fn press(&mut self, usage: u8) -> Result<()> {
        self.require_keyboard()?;
        self.keyboard.press(&self.device, usage)
    }

    /// Press and release a Win32 virtual-key code using the fixed
    /// [`vk_to_usage`] mapping, not layout-aware character translation.
    pub fn press_vk(&mut self, vk: u16) -> Result<()> {
        self.require_keyboard()?;
        let usage = vk_to_usage(vk).ok_or_else(|| {
            Error::Unsupported(format!("no HID usage mapping for virtual-key 0x{vk:04X}"))
        })?;
        self.press(usage)
    }

    /// Press a HID key combination in order, hold it for 20 ms, then release
    /// newly held keys in reverse order. Put modifiers such as Ctrl first.
    ///
    /// At most 14 entries are accepted, still subject to the six non-modifier
    /// slot limit including keys already held. Validation and sending proceed
    /// in order: an invalid later usage may follow input already delivered.
    /// On failure, cleanup is attempted without releasing preexisting keys.
    pub fn press_combo(&mut self, usages: &[u8]) -> Result<()> {
        self.require_keyboard()?;
        self.keyboard.press_combo(&self.device, usages)
    }

    /// Press a key combination using the fixed [`vk_to_usage`] mapping.
    /// Mapping all entries succeeds before any report is sent; the HID limits
    /// and cleanup behavior are those of [`Self::press_combo`].
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

    /// Hold a HID usage for a duration, blocking the calling thread, then release
    /// it unless it was already held. Cleanup is attempted if the release fails.
    pub fn hold_key(&mut self, usage: u8, duration: Duration) -> Result<()> {
        self.require_keyboard()?;
        self.keyboard.hold_key(&self.device, usage, duration)
    }

    /// Repeat a HID usage `count` times, holding it for 8 ms each time and
    /// sleeping for `interval` between presses (not between start times).
    ///
    /// Zero repetitions send nothing. A usage already held by this object
    /// remains held and does not generate repeated key-down edges.
    pub fn repeat_key(&mut self, usage: u8, count: u32, interval: Duration) -> Result<()> {
        self.require_keyboard()?;
        self.keyboard
            .repeat_key(&self.device, usage, count, interval)
    }

    /// Type printable ASCII using the US HID layout, plus tab and line breaks.
    ///
    /// CR and LF send Enter; CRLF sends one Enter. Other characters return
    /// [`Error::Unsupported`]. The target must use the US layout; this method
    /// does not switch its layout. Caps Lock is sampled once at the start.
    /// Characters are checked and sent in order, so an error can leave an
    /// already typed prefix. Previously held keys/modifiers remain active and
    /// can change the resulting input.
    pub fn type_ascii(&mut self, text: &str) -> Result<()> {
        self.require_keyboard()?;
        self.keyboard.type_ascii(&self.device, text)
    }

    /// Type text using a [`KeyboardLayout`] to map characters to HID reports.
    ///
    /// This does not switch the target application's layout: it must match the
    /// selected layout. Caps Lock is sampled once at the start. Only characters
    /// representable by a single mapped BMP key combination are supported; dead
    /// keys, multi-key composition and non-BMP characters are rejected.
    /// CR and LF send Enter; CRLF sends one Enter; tab sends Tab.
    ///
    /// Mapping and sending are incremental, so failure may leave a typed prefix.
    /// Held keys/modifiers are preserved and may alter the text received.
    pub fn type_text_layout(&mut self, text: &str, layout: KeyboardLayout) -> Result<()> {
        self.require_keyboard()?;
        self.keyboard.type_text_layout(&self.device, text, layout)
    }

    /// Send Unicode text as UTF-16 key-down/up pairs through Win32
    /// `SendInput` / `KEYEVENTF_UNICODE`, including surrogate pairs.
    ///
    /// This bypasses the Logitech driver and does not require the keyboard flag.
    /// Windows input restrictions and target application support still apply;
    /// success means events were inserted, not that text was accepted. Newlines
    /// are sent literally, without the HID path's Enter/CRLF conversion.
    /// On partial insertion a release is attempted, but input already sent
    /// (possibly only part of a character) is not rolled back.
    pub fn type_unicode(&mut self, text: &str) -> Result<()> {
        keyboard::type_unicode(text)
    }

    /// Type text with the selected mode. See [`Self::type_text_layout`] for HID
    /// layout/character limits and [`Self::type_unicode`] for the Win32 path.
    pub fn type_text(&mut self, text: &str, mode: TextMode) -> Result<()> {
        match mode {
            TextMode::Hid(layout) => self.type_text_layout(text, layout),
            TextMode::Win32Unicode => self.type_unicode(text),
        }
    }

    /// Send an empty keyboard report, clearing this object's held-key state
    /// after success. Does not release mouse buttons or physical keyboard keys.
    pub fn release_all(&mut self) -> Result<()> {
        self.require_keyboard()?;
        self.keyboard.release_all(&self.device)
    }

    /// Move by relative HID counts, not physical screen pixels. Positive X is
    /// right and positive Y is down; Windows pointer speed/acceleration applies.
    ///
    /// Large deltas are split into signed 16-bit reports with a 1 ms wait between
    /// reports. A failure may follow partial movement. A successful send does
    /// not prove movement occurred; the driver's PID gate can zero X/Y.
    pub fn mouse_move_relative(&mut self, dx: i32, dy: i32) -> Result<()> {
        self.require_mouse()?;
        self.mouse.move_relative(&self.device, dx, dy)
    }

    /// Hold a mouse button.
    pub fn mouse_button_down(&mut self, button: MouseButton) -> Result<()> {
        self.require_mouse()?;
        self.mouse.button_down(&self.device, button)
    }

    /// Release a mouse button, updating local state only after success.
    /// A failed release leaves the button held locally so it can be retried.
    pub fn mouse_button_up(&mut self, button: MouseButton) -> Result<()> {
        self.require_mouse()?;
        self.mouse.button_up(&self.device, button)
    }

    /// Hold a mouse button for 20 ms, then release it unless this object already
    /// held it. An already held button does not generate a new click edge.
    pub fn mouse_click(&mut self, button: MouseButton) -> Result<()> {
        self.require_mouse()?;
        self.mouse.click(&self.device, button)
    }

    /// Perform two [`Self::mouse_click`] calls with a 40 ms wait between them.
    /// Recognition as a double-click depends on Windows/application settings.
    pub fn mouse_double_click(&mut self, button: MouseButton) -> Result<()> {
        self.require_mouse()?;
        self.mouse.click(&self.device, button)?;
        sleep(Duration::from_millis(40));
        self.mouse.click(&self.device, button)
    }

    /// Turn the vertical wheel by signed HID wheel units (typically notches),
    /// not Win32 `WHEEL_DELTA` multiples of 120. Positive means away from the user;
    /// application scroll distance depends on Windows/application settings.
    pub fn mouse_wheel(&mut self, delta: i8) -> Result<()> {
        self.require_mouse()?;
        self.mouse.wheel(&self.device, delta)
    }

    /// Move to physical screen coordinates `(x, y)` and click a button.
    /// No click is sent if positioning fails; this does not focus a window first.
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

    /// Move to physical screen coordinates `(x, y)` and double-click a button.
    /// No click is sent if positioning fails.
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

    /// Press at `from`, move to `to` (both physical screen coordinates), and
    /// attempt to release the button introduced by this operation, including on
    /// failure. A previously held button stays held. Cleanup failure can leave
    /// the button held; cursor movement is never rolled back.
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

    /// Convert physical client-relative coordinates to screen coordinates and
    /// click. `window` is a borrowed Win32 HWND; it is not activated or focused,
    /// and the point is not checked against the client bounds or occluding windows.
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

    /// Request removal of a shared virtual device (`DEVICE_KIND_KEYBOARD` or
    /// `DEVICE_KIND_MOUSE`), even if it was not selected in this object's options.
    ///
    /// Windows `ERROR_INVALID_PARAMETER` is treated as success (including a
    /// missing device). This neither waits for PnP disappearance nor updates
    /// local input state/options. It can affect other clients; drop does not
    /// call this method.
    pub fn remove_device(&self, kind: u32) -> Result<()> {
        self.device.remove_device(kind)
    }

    /// Set the absolute cursor position using the virtual HID mouse.
    ///
    /// Coordinates are physical screen pixels on the virtual desktop and are
    /// clamped to its bounding rectangle, which may have a negative origin.
    /// The relative-only HID path probes movement, then performs at most 200
    /// corrections of up to 64 counts per axis with 10 ms waits. It can fail on
    /// the PID gate, stalling or an unreachable target, after moving the cursor.
    /// There is no Win32 fallback; success requires observing the clamped target.
    pub fn set_cursor_pos_driver(&mut self, x: i32, y: i32) -> Result<()> {
        self.require_mouse()?;
        cursor::set_cursor_pos_driver(&self.device, &mut self.mouse, x, y)
    }

    /// Set the absolute cursor position with Win32 `SendInput` using
    /// `MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK`.
    ///
    /// Coordinates are physical screen pixels, clamped to the virtual desktop's
    /// bounding rectangle. This explicit Win32 path needs no open mouse and has
    /// no Logitech fallback. Success reports event insertion, not verified arrival.
    pub fn set_cursor_pos_win32(x: i32, y: i32) -> Result<()> {
        cursor::set_cursor_pos_win32(x, y)
    }

    /// Set the absolute cursor position with Win32 `SetCursorPos`.
    ///
    /// Coordinates are physical screen pixels, clamped to the virtual desktop's
    /// bounding rectangle. This needs no open mouse and moves the cursor directly
    /// rather than using `SendInput`; arrival is not subsequently verified.
    pub fn set_cursor_pos_direct(x: i32, y: i32) -> Result<()> {
        cursor::set_cursor_pos_direct(x, y)
    }

    /// Set the absolute cursor position in physical screen pixels using the
    /// selected mode. All modes clamp to the virtual desktop's bounding rectangle;
    /// only [`AbsoluteMode::Driver`] requires this object's mouse flag.
    pub fn set_cursor_pos(&mut self, x: i32, y: i32, mode: AbsoluteMode) -> Result<()> {
        match mode {
            AbsoluteMode::Driver => self.set_cursor_pos_driver(x, y),
            AbsoluteMode::Win32 => Self::set_cursor_pos_win32(x, y),
            AbsoluteMode::SetCursorPos => Self::set_cursor_pos_direct(x, y),
        }
    }
}
