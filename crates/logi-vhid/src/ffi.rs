//! Raw Win32 bindings and small helpers.
//!
//! Win32 declarations and ABI layouts are centralized here. Modules that call
//! these functions still contain the required `unsafe` blocks.

use crate::error::{Error, Result};

pub(crate) type HANDLE = isize;
pub(crate) type BOOL = i32;
pub(crate) type DWORD = u32;

pub(crate) const INVALID_HANDLE_VALUE: HANDLE = -1;
pub(crate) const GENERIC_READ: DWORD = 0x8000_0000;
pub(crate) const GENERIC_WRITE: DWORD = 0x4000_0000;
pub(crate) const FILE_SHARE_READ: DWORD = 0x0000_0001;
pub(crate) const FILE_SHARE_WRITE: DWORD = 0x0000_0002;
pub(crate) const OPEN_EXISTING: DWORD = 3;
pub(crate) const DIGCF_PRESENT: DWORD = 0x0000_0002;
pub(crate) const DIGCF_DEVICEINTERFACE: DWORD = 0x0000_0010;

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct Guid {
    pub(crate) data1: u32,
    pub(crate) data2: u16,
    pub(crate) data3: u16,
    pub(crate) data4: [u8; 8],
}

pub(crate) const VIRTUAL_BUS_GUID: Guid = Guid {
    data1: 0xDFBE_DCDD,
    data2: 0x2148,
    data3: 0x416D,
    data4: [0x9E, 0x4D, 0xCE, 0xCC, 0x24, 0x24, 0x12, 0x8C],
};

pub(crate) const HID_GUID: Guid = Guid {
    data1: 0x4D1E_55B2,
    data2: 0xF16F,
    data3: 0x11CF,
    data4: [0x88, 0xCB, 0x00, 0x11, 0x11, 0x00, 0x00, 0x30],
};

#[repr(C)]
pub(crate) struct SpDeviceInterfaceData {
    pub(crate) cb_size: DWORD,
    pub(crate) interface_class_guid: Guid,
    pub(crate) flags: DWORD,
    pub(crate) reserved: usize,
}

/// A point in physical pixels for this crate's coordinate APIs. Screen points
/// use the virtual-desktop origin (coordinates may be negative); client points
/// use the window's client origin. The calling API determines which space applies.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

/// A pixel rectangle with inclusive left/top and exclusive right/bottom edges.
/// Screen rectangles may have negative origins; client rectangles are local to
/// the window. See the returning API for the coordinate space.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn width(self) -> i32 {
        self.right - self.left
    }

    pub fn height(self) -> i32 {
        self.bottom - self.top
    }

    /// Test membership using inclusive left/top and exclusive right/bottom edges.
    pub fn contains(self, point: Point) -> bool {
        point.x >= self.left && point.x < self.right && point.y >= self.top && point.y < self.bottom
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct MonitorInfoRaw {
    pub(crate) cb_size: DWORD,
    pub(crate) monitor: Rect,
    pub(crate) work: Rect,
    pub(crate) flags: DWORD,
}

#[repr(C)]
pub(crate) struct MouseInput {
    pub(crate) dx: i32,
    pub(crate) dy: i32,
    pub(crate) mouse_data: u32,
    pub(crate) flags: u32,
    pub(crate) time: u32,
    pub(crate) extra_info: usize,
}

#[repr(C)]
pub(crate) struct Input {
    pub(crate) type_: u32,
    pub(crate) mi: MouseInput,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct KeyInput {
    pub(crate) w_vk: u16,
    pub(crate) w_scan: u16,
    pub(crate) flags: u32,
    pub(crate) time: u32,
    pub(crate) extra_info: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct InputKeyboard {
    pub(crate) type_: u32,
    pub(crate) _pad: u32,
    pub(crate) ki: KeyInput,
    pub(crate) _tail: [u8; 8],
}

pub(crate) const INPUT_MOUSE: u32 = 0;
pub(crate) const INPUT_KEYBOARD: u32 = 1;
pub(crate) const KEYEVENTF_KEYUP: u32 = 0x0002;
pub(crate) const KEYEVENTF_UNICODE: u32 = 0x0004;
pub(crate) const MOUSEEVENTF_MOVE: u32 = 0x0001;
pub(crate) const MOUSEEVENTF_ABSOLUTE: u32 = 0x8000;
pub(crate) const MOUSEEVENTF_VIRTUALDESK: u32 = 0x4000;
pub(crate) const MONITOR_DEFAULTTONEAREST: DWORD = 2;
pub(crate) const MONITORINFOF_PRIMARY: DWORD = 1;
const SM_XVIRTUALSCREEN: i32 = 76;
const SM_YVIRTUALSCREEN: i32 = 77;
const SM_CXVIRTUALSCREEN: i32 = 78;
const SM_CYVIRTUALSCREEN: i32 = 79;

#[link(name = "setupapi")]
extern "system" {
    pub(crate) fn SetupDiGetClassDevsW(
        class_guid: *const Guid,
        enumerator: *const u16,
        hwnd_parent: HANDLE,
        flags: DWORD,
    ) -> HANDLE;
    pub(crate) fn SetupDiEnumDeviceInterfaces(
        device_info_set: HANDLE,
        device_info_data: *const u8,
        interface_class_guid: *const Guid,
        member_index: DWORD,
        device_interface_data: *mut SpDeviceInterfaceData,
    ) -> BOOL;
    pub(crate) fn SetupDiGetDeviceInterfaceDetailW(
        device_info_set: HANDLE,
        device_interface_data: *const SpDeviceInterfaceData,
        detail: *mut u8,
        detail_size: DWORD,
        required_size: *mut DWORD,
        device_info_data: *mut u8,
    ) -> BOOL;
    pub(crate) fn SetupDiDestroyDeviceInfoList(device_info_set: HANDLE) -> BOOL;
}

#[link(name = "kernel32")]
extern "system" {
    pub(crate) fn CreateFileW(
        file_name: *const u16,
        desired_access: DWORD,
        share_mode: DWORD,
        security_attributes: *const u8,
        creation_disposition: DWORD,
        flags_and_attributes: DWORD,
        template_file: HANDLE,
    ) -> HANDLE;
    pub(crate) fn DeviceIoControl(
        device: HANDLE,
        io_control_code: DWORD,
        in_buffer: *const u8,
        in_size: DWORD,
        out_buffer: *mut u8,
        out_size: DWORD,
        bytes_returned: *mut DWORD,
        overlapped: *mut u8,
    ) -> BOOL;
    pub(crate) fn CloseHandle(object: HANDLE) -> BOOL;
}

#[link(name = "user32")]
extern "system" {
    pub(crate) fn GetCursorPos(point: *mut Point) -> BOOL;
    pub(crate) fn SetCursorPos(x: i32, y: i32) -> BOOL;
    pub(crate) fn GetSystemMetrics(index: i32) -> i32;
    pub(crate) fn SendInput(count: u32, inputs: *const u8, size: i32) -> u32;
    pub(crate) fn GetAsyncKeyState(virtual_key: i32) -> i16;
    pub(crate) fn GetKeyState(virtual_key: i32) -> i16;

    pub(crate) fn GetForegroundWindow() -> isize;
    pub(crate) fn GetWindowRect(window: isize, rect: *mut Rect) -> BOOL;
    pub(crate) fn GetClientRect(window: isize, rect: *mut Rect) -> BOOL;
    pub(crate) fn ClientToScreen(window: isize, point: *mut Point) -> BOOL;
    pub(crate) fn ScreenToClient(window: isize, point: *mut Point) -> BOOL;
    pub(crate) fn GetWindowTextLengthW(window: isize) -> i32;
    pub(crate) fn GetWindowTextW(window: isize, text: *mut u16, max_count: i32) -> i32;
    pub(crate) fn IsWindowVisible(window: isize) -> BOOL;
    pub(crate) fn EnumWindows(
        callback: Option<unsafe extern "system" fn(isize, isize) -> BOOL>,
        data: isize,
    ) -> BOOL;
    pub(crate) fn GetDpiForWindow(window: isize) -> u32;

    pub(crate) fn MonitorFromPoint(point: Point, flags: DWORD) -> isize;
    pub(crate) fn GetMonitorInfoW(monitor: isize, info: *mut MonitorInfoRaw) -> BOOL;
    pub(crate) fn EnumDisplayMonitors(
        hdc: isize,
        clip: *const Rect,
        callback: Option<unsafe extern "system" fn(isize, isize, *mut Rect, isize) -> BOOL>,
        data: isize,
    ) -> BOOL;

    pub(crate) fn VkKeyScanExW(ch: u16, layout: isize) -> i16;
    pub(crate) fn LoadKeyboardLayoutW(name: *const u16, flags: u32) -> isize;
    pub(crate) fn MapVirtualKeyExW(code: u32, map_type: u32, layout: isize) -> u32;
    pub(crate) fn ToUnicodeEx(
        virtual_key: u32,
        scan_code: u32,
        key_state: *const u8,
        buffer: *mut u16,
        buffer_size: i32,
        flags: u32,
        layout: isize,
    ) -> i32;

    fn SetThreadDpiAwarenessContext(value: isize) -> isize;
}

/// A thread-affine DPI override. The saved context is restored on early
/// returns and unwinding, without changing the host process's DPI policy.
struct PhysicalDpiContext {
    previous: isize,
    _thread_affine: std::marker::PhantomData<*mut ()>,
}

impl PhysicalDpiContext {
    fn enter() -> Result<Self> {
        // DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2.
        let previous = unsafe { SetThreadDpiAwarenessContext(-4) };
        if previous == 0 {
            return Err(Error::last_io("SetThreadDpiAwarenessContext"));
        }
        Ok(Self {
            previous,
            _thread_affine: std::marker::PhantomData,
        })
    }

    fn restore(&mut self) -> Result<()> {
        if unsafe { SetThreadDpiAwarenessContext(self.previous) } == 0 {
            return Err(Error::last_io("SetThreadDpiAwarenessContext (restore)"));
        }
        self.previous = 0;
        Ok(())
    }
}

impl Drop for PhysicalDpiContext {
    fn drop(&mut self) {
        if self.previous != 0 {
            unsafe { SetThreadDpiAwarenessContext(self.previous) };
        }
    }
}

pub(crate) fn with_physical_coordinates<T>(operation: impl FnOnce() -> Result<T>) -> Result<T> {
    let mut context = PhysicalDpiContext::enter()?;
    let result = operation();
    let restored = context.restore();
    // Keep the original operation error if both the query and restoration fail.
    match result {
        Ok(value) => restored.map(|()| value),
        Err(error) => Err(error),
    }
}

/// Bounding box of the virtual desktop in physical pixels. Call only inside
/// `with_physical_coordinates`, so queries and their consumers share a scope.
pub(crate) fn virtual_screen() -> Result<(i32, i32, i32, i32)> {
    let (x, y, width, height) = unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    };
    if width <= 0
        || height <= 0
        || x.checked_add(width).is_none()
        || y.checked_add(height).is_none()
    {
        // GetSystemMetrics does not expose an extended last-error value.
        return Err(Error::io(
            "GetSystemMetrics",
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "invalid virtual desktop bounds",
            ),
        ));
    }
    Ok((x, y, width, height))
}
