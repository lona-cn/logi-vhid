//! C ABI for a subset of the Rust API; declarations are in `include/logi_vhid.h`.
//!
//! Open functions return a handle or null; input functions return 0 or -1.
//! [`logi_vhid_pid_gate`] returns 1, 0, or -1; path and error queries return
//! library-owned string pointers. [`logi_vhid_close`] has no return value.
//! [`logi_vhid_last_error`] reports the calling thread's most recent error.
//! Calls are synchronous. Serialize every use of the same handle, including
//! queries and close. Separate handles still target shared virtual devices.

use std::cell::RefCell;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::ptr::null_mut;

use crate::{AbsoluteMode, LogitechVirtualHid, MouseButton, OpenOptions};

/// Opaque handle returned by `logi_vhid_open`.
pub struct LogiVhidHandle {
    device: LogitechVirtualHid,
    path: CString,
}

thread_local! {
    static LAST_ERROR: RefCell<Option<CString>> = const { RefCell::new(None) };
}

fn set_error(message: impl std::fmt::Display) {
    let mut message = message.to_string().replace('\0', " ");
    if message.is_empty() {
        message.push_str("unknown error");
    }
    LAST_ERROR.with(|slot| {
        *slot.borrow_mut() = Some(CString::new(message).expect("NUL bytes replaced"));
    });
}

fn fail(message: impl std::fmt::Display) -> i32 {
    set_error(message);
    -1
}

fn button_from_u8(button: u8) -> Option<MouseButton> {
    match button {
        0 => Some(MouseButton::Left),
        1 => Some(MouseButton::Right),
        2 => Some(MouseButton::Middle),
        3 => Some(MouseButton::Back),
        4 => Some(MouseButton::Forward),
        _ => None,
    }
}

fn mode_from_u8(mode: u8) -> Option<AbsoluteMode> {
    match mode {
        0 => Some(AbsoluteMode::Driver),
        1 => Some(AbsoluteMode::Win32),
        2 => Some(AbsoluteMode::SetCursorPos),
        _ => None,
    }
}

fn open_with_options(options: OpenOptions) -> *mut LogiVhidHandle {
    match LogitechVirtualHid::open_with_options(options) {
        Ok(device) => {
            let path =
                CString::new(device.device_path()).unwrap_or_else(|_| CString::new("").unwrap());
            Box::into_raw(Box::new(LogiVhidHandle { device, path }))
        }
        Err(error) => {
            set_error(error);
            null_mut()
        }
    }
}

/// Open the virtual bus with both keyboard and mouse; return null on failure.
/// Read [`logi_vhid_last_error`] on the calling thread for the failure reason.
#[no_mangle]
pub extern "C" fn logi_vhid_open() -> *mut LogiVhidHandle {
    open_with_options(OpenOptions::both())
}

/// Open with selected devices: zero disables each device, nonzero enables it.
/// Both may be zero to open only the control bus. Return null on failure.
/// Win32 text/cursor modes do not require a device flag, but all open calls
/// still require the G HUB bus. Failed opens may leave children already created.
#[no_mangle]
pub extern "C" fn logi_vhid_open_options(keyboard: u8, mouse: u8) -> *mut LogiVhidHandle {
    open_with_options(OpenOptions {
        keyboard: keyboard != 0,
        mouse: mouse != 0,
    })
}

/// Close a handle created by `logi_vhid_open` or `logi_vhid_open_options`.
/// Closing only drops the control handle; it does not send release reports or
/// remove the shared virtual devices. Closing null is a no-op.
///
/// # Safety
/// `handle` must be null or a live pointer returned by this library, closed
/// exactly once. The caller must serialize all operations on this handle,
/// including queries and close, and finish reading its path before closing it.
#[no_mangle]
pub unsafe extern "C" fn logi_vhid_close(handle: *mut LogiVhidHandle) {
    if !handle.is_null() {
        drop(Box::from_raw(handle));
    }
}

/// Latest error text for the calling thread, owned by the library. Initially
/// empty; successful calls do not clear it. The pointer is valid until the next
/// library error on this thread or this thread exits. Errors on other threads do
/// not change or invalidate it. Do not modify or free the string.
#[no_mangle]
pub extern "C" fn logi_vhid_last_error() -> *const c_char {
    LAST_ERROR.with(|slot| {
        slot.borrow()
            .as_ref()
            .map_or(c"".as_ptr(), |message| message.as_ptr())
    })
}

/// Library-owned UTF-8 control-device path, valid until the handle is closed.
/// A null handle returns null without updating the calling thread's error.
///
/// # Safety
/// `handle` must be null or live. The caller must serialize all operations on the same
/// handle, including queries and close. The returned string is library-owned:
/// do not modify or free it, and finish reading or copying it before close.
#[no_mangle]
pub unsafe extern "C" fn logi_vhid_device_path(handle: *mut LogiVhidHandle) -> *const c_char {
    handle
        .as_ref()
        .map(|handle| handle.path.as_ptr())
        .unwrap_or(std::ptr::null())
}

/// Move by relative HID counts, not guaranteed screen pixels.
/// Windows pointer settings apply; success does not prove motion if the PID
/// gate is active. Return 0 on success or -1 on error, including a null handle.
///
/// # Safety
/// `handle` must be null or live. The caller must serialize all operations on the same
/// handle, including queries and close.
#[no_mangle]
pub unsafe extern "C" fn logi_vhid_move(handle: *mut LogiVhidHandle, dx: i32, dy: i32) -> i32 {
    let Some(handle) = handle.as_mut() else {
        return fail("null handle");
    };
    match handle.device.mouse_move_relative(dx, dy) {
        Ok(()) => 0,
        Err(error) => fail(error),
    }
}

/// Click at the current cursor position: press and release the button.
/// Buttons: 0 left, 1 right, 2 middle, 3 back, 4 forward.
/// Return 0 on success or -1 on error, including null handle/invalid button.
///
/// # Safety
/// `handle` must be null or live. The caller must serialize all operations on the same
/// handle, including queries and close.
#[no_mangle]
pub unsafe extern "C" fn logi_vhid_click(handle: *mut LogiVhidHandle, button: u8) -> i32 {
    let Some(handle) = handle.as_mut() else {
        return fail("null handle");
    };
    let Some(button) = button_from_u8(button) else {
        return fail(format!("unknown mouse button {button}"));
    };
    match handle.device.mouse_click(button) {
        Ok(()) => 0,
        Err(error) => fail(error),
    }
}

/// Click twice at the current cursor position, using the same button values as
/// [`logi_vhid_click`]. Recognition depends on Windows/application settings.
/// Return 0 on success or -1 on error, including null handle/invalid button.
///
/// # Safety
/// `handle` must be null or live. The caller must serialize all operations on the same
/// handle, including queries and close.
#[no_mangle]
pub unsafe extern "C" fn logi_vhid_double_click(handle: *mut LogiVhidHandle, button: u8) -> i32 {
    let Some(handle) = handle.as_mut() else {
        return fail("null handle");
    };
    let Some(button) = button_from_u8(button) else {
        return fail(format!("unknown mouse button {button}"));
    };
    match handle.device.mouse_double_click(button) {
        Ok(()) => 0,
        Err(error) => fail(error),
    }
}

/// Press and release one HID keyboard usage.
/// The usage is from HID Keyboard/Keypad page 0x07, not a Win32 virtual key
/// or character code. Return 0 on success or -1 on error, including null handle.
///
/// # Safety
/// `handle` must be null or live. The caller must serialize all operations on the same
/// handle, including queries and close.
#[no_mangle]
pub unsafe extern "C" fn logi_vhid_key(handle: *mut LogiVhidHandle, usage: u8) -> i32 {
    let Some(handle) = handle.as_mut() else {
        return fail("null handle");
    };
    match handle.device.press(usage) {
        Ok(()) => 0,
        Err(error) => fail(error),
    }
}

unsafe fn c_text<'a>(text: *const c_char) -> Result<&'a str, i32> {
    if text.is_null() {
        return Err(fail("null text pointer"));
    }
    CStr::from_ptr(text)
        .to_str()
        .map_err(|error| fail(format!("text is not UTF-8: {error}")))
}

/// Type printable ASCII plus tab, CR and LF through the US-layout virtual
/// keyboard. CRLF produces one Enter; the target's layout must already be US.
/// Unsupported text may fail after typing a prefix. Return 0 on success or -1
/// on error, including null handle/text and invalid UTF-8.
///
/// # Safety
/// `handle` must be null or live. The caller must serialize all operations on the
/// same handle, including queries and close. `text` must be null or point to
/// readable, NUL-terminated UTF-8 bytes that remain valid and unchanged for the
/// entire call. The library does not retain `text`; the first NUL ends it.
#[no_mangle]
pub unsafe extern "C" fn logi_vhid_type_ascii(
    handle: *mut LogiVhidHandle,
    text: *const c_char,
) -> i32 {
    let Some(handle) = handle.as_mut() else {
        return fail("null handle");
    };
    let text = match c_text(text) {
        Ok(text) => text,
        Err(code) => return code,
    };
    match handle.device.type_ascii(text) {
        Ok(()) => 0,
        Err(error) => fail(error),
    }
}

/// Type UTF-8 text as Win32 `SendInput` Unicode events, not virtual HID reports.
/// Non-BMP text uses UTF-16 surrogate pairs. CR/LF/tab are literal Unicode events,
/// not HID Enter/Tab presses. Return 0 on success or -1 on error; success means
/// events were inserted, not that the target accepted them. Partial input is not
/// rolled back. A null handle/text or invalid UTF-8 returns -1.
///
/// # Safety
/// `handle` must be null or live. The caller must serialize all operations on the
/// same handle, including queries and close. `text` must be null or point to
/// readable, NUL-terminated UTF-8 bytes that remain valid and unchanged for the
/// entire call. The library does not retain `text`; the first NUL ends it.
#[no_mangle]
pub unsafe extern "C" fn logi_vhid_type_unicode(
    handle: *mut LogiVhidHandle,
    text: *const c_char,
) -> i32 {
    let Some(handle) = handle.as_mut() else {
        return fail("null handle");
    };
    let text = match c_text(text) {
        Ok(text) => text,
        Err(code) => return code,
    };
    match handle.device.type_unicode(text) {
        Ok(()) => 0,
        Err(error) => fail(error),
    }
}

/// Set the cursor in physical screen pixels, clamped to the virtual desktop
/// bounding rectangle. Mode: 0 driver, 1 Win32 `SendInput`, 2 `SetCursorPos`.
/// Driver mode verifies arrival and has no fallback; Win32 modes do not verify
/// arrival. Return 0 on success or -1 on error, including null handle/invalid mode.
///
/// # Safety
/// `handle` must be null or live. The caller must serialize all operations on the same
/// handle, including queries and close.
#[no_mangle]
pub unsafe extern "C" fn logi_vhid_set_cursor(
    handle: *mut LogiVhidHandle,
    x: i32,
    y: i32,
    mode: u8,
) -> i32 {
    let Some(handle) = handle.as_mut() else {
        return fail("null handle");
    };
    let Some(mode) = mode_from_u8(mode) else {
        return fail(format!("unknown absolute mode {mode}"));
    };
    match handle.device.set_cursor_pos(x, y, mode) {
        Ok(()) => 0,
        Err(error) => fail(error),
    }
}

/// Whether the mouse PID gate is active: 1 yes, 0 no, -1 error.
/// An active gate zeroes relative mouse X/Y; this query does not change it.
///
/// # Safety
/// `handle` must be null or live. The caller must serialize all operations on the same
/// handle, including queries and close.
#[no_mangle]
pub unsafe extern "C" fn logi_vhid_pid_gate(handle: *mut LogiVhidHandle) -> i32 {
    let Some(handle) = handle.as_ref() else {
        return fail("null handle");
    };
    match handle.device.is_pid_gate_active() {
        Ok(active) => i32::from(active),
        Err(error) => fail(error),
    }
}

#[cfg(test)]
mod tests {
    use super::{logi_vhid_last_error, set_error};
    use std::ffi::CStr;
    use std::sync::mpsc;
    use std::thread;

    #[test]
    fn last_error_is_isolated_between_threads() {
        let (first_ready, wait_first) = mpsc::channel();
        let (second_ready, wait_second) = mpsc::channel();
        let first = thread::spawn(move || {
            set_error("first thread error");
            first_ready.send(()).unwrap();
            wait_second.recv().unwrap();
            // Fetch a fresh pointer only after the other thread has finished
            // writing; never dereference a pointer invalidated by the old code.
            let observed = unsafe { CStr::from_ptr(logi_vhid_last_error()).to_owned() };
            set_error("first thread replacement");
            first_ready.send(()).unwrap();
            wait_second.recv().unwrap();
            observed
        });
        let second = thread::spawn(move || {
            wait_first.recv().unwrap();
            set_error("second thread error");
            second_ready.send(()).unwrap();
            wait_first.recv().unwrap();
            let observed = unsafe { CStr::from_ptr(logi_vhid_last_error()).to_owned() };
            second_ready.send(()).unwrap();
            observed
        });

        let first_error = first.join().unwrap();
        let second_error = second.join().unwrap();
        assert_eq!(first_error.to_bytes(), b"first thread error");
        assert_eq!(second_error.to_bytes(), b"second thread error");
    }
}
