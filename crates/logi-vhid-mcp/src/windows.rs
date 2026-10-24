use std::cell::Cell;
use std::ptr::{null, null_mut};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, SetLastError, HANDLE, HWND, POINT, WAIT_ABANDONED, WAIT_OBJECT_0,
    WAIT_TIMEOUT,
};
use windows_sys::Win32::System::RemoteDesktop::{
    ProcessIdToSessionId, WTSActive, WTSConnectState, WTSFreeMemory, WTSQuerySessionInformationW,
    WTS_CONNECTSTATE_CLASS, WTS_CURRENT_SERVER_HANDLE,
};
use windows_sys::Win32::System::StationsAndDesktops::{
    CloseDesktop, GetUserObjectInformationW, OpenInputDesktop, DESKTOP_READOBJECTS, HDESK, UOI_NAME,
};
use windows_sys::Win32::System::Threading::{
    CreateMutexW, GetCurrentProcessId, ReleaseMutex, WaitForSingleObject,
};
use windows_sys::Win32::UI::HiDpi::{
    SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetAncestor, GetForegroundWindow, GetWindowTextLengthW, GetWindowTextW,
    GetWindowThreadProcessId, IsWindow, WindowFromPoint, GA_ROOT,
};

use crate::params::{
    CoordinateSpace, DesktopInfo, ErrorCode, MonitorDto, NativeError, NativeKind, PointDto,
    PointSpec, RectDto, TargetWindow, ToolFailure, WindowDto,
};

// Pure policy evaluation is deliberately separate from the fallible OS snapshot.
// No successful safety decision is manufactured when an API query fails.
struct SafetySnapshot {
    target_exists: bool,
    target_pid: u32,
    foreground_hwnd: usize,
    foreground_pid: u32,
    current_session: u32,
    target_session: u32,
    session_active: bool,
    default_desktop: bool,
}

fn check_safety(
    snapshot: &SafetySnapshot,
    expected_hwnd: usize,
    expected_pid: u32,
) -> Result<(), ToolFailure> {
    if !snapshot.target_exists || snapshot.target_pid != expected_pid {
        return Err(ToolFailure::new(
            ErrorCode::TargetChanged,
            "target window no longer exists or belongs to another process",
        ));
    }
    if snapshot.current_session != snapshot.target_session
        || !snapshot.session_active
        || !snapshot.default_desktop
    {
        return Err(ToolFailure::new(
            ErrorCode::DesktopUnavailable,
            "input requires the active session and its Default input desktop",
        ));
    }
    if snapshot.foreground_hwnd != expected_hwnd || snapshot.foreground_pid != expected_pid {
        return Err(ToolFailure::new(
            ErrorCode::TargetChanged,
            "target is no longer the foreground window",
        ));
    }
    Ok(())
}

fn contains(rect: &RectDto, point: &PointDto) -> bool {
    point.x >= rect.left && point.x < rect.right && point.y >= rect.top && point.y < rect.bottom
}

fn check_client_point(point: &PointDto, client: &RectDto) -> Result<(), ToolFailure> {
    if contains(client, point) {
        Ok(())
    } else {
        Err(ToolFailure::new(
            ErrorCode::InvalidArguments,
            "client point is outside the target client rectangle",
        ))
    }
}

fn check_screen_point(
    point: &PointDto,
    monitors: impl Iterator<Item = RectDto>,
) -> Result<(), ToolFailure> {
    if monitors.into_iter().any(|rect| contains(&rect, point)) {
        Ok(())
    } else {
        Err(ToolFailure::new(
            ErrorCode::InvalidArguments,
            "screen point is outside every physical monitor",
        ))
    }
}

fn is_default_desktop(name: &[u16]) -> bool {
    const DEFAULT: &[u8] = b"default";
    name.len() == DEFAULT.len()
        && name
            .iter()
            .zip(DEFAULT)
            .all(|(&unit, &expected)| unit <= 0x7f && (unit as u8).eq_ignore_ascii_case(&expected))
}

fn os_failure(code: ErrorCode, context: &'static str, os_code: u32) -> ToolFailure {
    // A zero error after a failed API call is uncertainty, not success or proof
    // of UIPI. Keep the original numeric value without its misleading OS text.
    let message = if os_code == 0 {
        "API failed without an extended OS error code; cause is unknown".to_owned()
    } else {
        std::io::Error::from_raw_os_error(os_code as i32).to_string()
    };
    ToolFailure {
        code,
        message: format!("{context}: {message}"),
        native: Some(NativeError {
            kind: NativeKind::Io,
            context: Some(context.to_owned()),
            os_code: Some(os_code as i32),
            message,
        }),
    }
}

fn last_failure(code: ErrorCode, context: &'static str) -> ToolFailure {
    os_failure(code, context, unsafe { GetLastError() })
}

fn core_query_error(error: logi_vhid::Error) -> ToolFailure {
    if let logi_vhid::Error::Io { context, source } = &error {
        if let Some(code) = source.raw_os_error() {
            return os_failure(ErrorCode::NativeError, context, code as u32);
        }
    }
    let mut failure = ToolFailure::from_core(&error);
    failure.code = ErrorCode::NativeError;
    failure
}

fn rect_dto(rect: logi_vhid::Rect) -> RectDto {
    RectDto {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    }
}

fn session_id(pid: u32) -> Result<u32, ToolFailure> {
    let mut session = 0;
    if unsafe { ProcessIdToSessionId(pid, &mut session) } == 0 {
        return Err(last_failure(
            ErrorCode::DesktopUnavailable,
            "ProcessIdToSessionId",
        ));
    }
    Ok(session)
}

fn session_active(session: u32) -> Result<bool, ToolFailure> {
    let mut buffer = null_mut();
    let mut bytes = 0;
    if unsafe {
        WTSQuerySessionInformationW(
            WTS_CURRENT_SERVER_HANDLE,
            session,
            WTSConnectState,
            &mut buffer,
            &mut bytes,
        )
    } == 0
    {
        return Err(last_failure(
            ErrorCode::DesktopUnavailable,
            "WTSQuerySessionInformationW",
        ));
    }
    // This allocation belongs to WTS, including on malformed successful replies.
    let valid =
        !buffer.is_null() && bytes as usize >= std::mem::size_of::<WTS_CONNECTSTATE_CLASS>();
    let active = valid
        && unsafe { std::ptr::read_unaligned(buffer.cast::<WTS_CONNECTSTATE_CLASS>()) }
            == WTSActive;
    if !buffer.is_null() {
        unsafe { WTSFreeMemory(buffer.cast()) };
    }
    if !valid {
        return Err(ToolFailure::new(
            ErrorCode::DesktopUnavailable,
            "WTSQuerySessionInformationW returned an incomplete connection state",
        ));
    }
    Ok(active)
}

struct DesktopHandle(HDESK);

impl Drop for DesktopHandle {
    fn drop(&mut self) {
        unsafe { CloseDesktop(self.0) };
    }
}

struct DesktopName {
    units: [u16; 256],
    len: usize,
}

impl DesktopName {
    fn query() -> Result<Self, ToolFailure> {
        let handle = unsafe { OpenInputDesktop(0, 0, DESKTOP_READOBJECTS) };
        if handle.is_null() {
            return Err(last_failure(
                ErrorCode::DesktopUnavailable,
                "OpenInputDesktop",
            ));
        }
        let desktop = DesktopHandle(handle);
        let mut name = Self {
            units: [0; 256],
            len: 0,
        };
        let mut needed = 0;
        if unsafe {
            GetUserObjectInformationW(
                desktop.0,
                UOI_NAME,
                name.units.as_mut_ptr().cast(),
                std::mem::size_of_val(&name.units) as u32,
                &mut needed,
            )
        } == 0
        {
            return Err(last_failure(
                ErrorCode::DesktopUnavailable,
                "GetUserObjectInformationW(UOI_NAME)",
            ));
        }
        let units = needed as usize / std::mem::size_of::<u16>();
        if needed % 2 != 0 || units == 0 || units > name.units.len() || name.units[units - 1] != 0 {
            return Err(ToolFailure::new(
                ErrorCode::DesktopUnavailable,
                "input desktop name was not a complete terminated UTF-16 value",
            ));
        }
        name.len = units - 1;
        Ok(name)
    }

    fn is_default(&self) -> bool {
        is_default_desktop(&self.units[..self.len])
    }
}

struct PhysicalPixels(DPI_AWARENESS_CONTEXT);

impl PhysicalPixels {
    fn enter() -> Result<Self, ToolFailure> {
        let previous =
            unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
        if previous.is_null() {
            return Err(last_failure(
                ErrorCode::NativeError,
                "SetThreadDpiAwarenessContext",
            ));
        }
        Ok(Self(previous))
    }
}

impl Drop for PhysicalPixels {
    fn drop(&mut self) {
        if unsafe { SetThreadDpiAwarenessContext(self.0) }.is_null() {
            eprintln!(
                "failed to restore thread DPI context: OS error {}",
                unsafe { GetLastError() }
            );
        }
    }
}

fn window_pid(hwnd: HWND) -> Result<u32, ToolFailure> {
    let mut pid = 0;
    if unsafe { GetWindowThreadProcessId(hwnd, &mut pid) } == 0 || pid == 0 {
        return Err(last_failure(
            ErrorCode::NativeError,
            "GetWindowThreadProcessId",
        ));
    }
    Ok(pid)
}

fn verify_identity(hwnd: HWND, pid: u32) -> Result<(), ToolFailure> {
    if unsafe { IsWindow(hwnd) } == 0 {
        return Err(ToolFailure::new(
            ErrorCode::TargetChanged,
            "target window no longer exists",
        ));
    }
    if window_pid(hwnd)? != pid {
        return Err(ToolFailure::new(
            ErrorCode::TargetChanged,
            "target window belongs to another process",
        ));
    }
    Ok(())
}

pub(crate) struct TargetGuard {
    // HWND and Cell intentionally keep this guard local to its worker thread.
    hwnd: HWND,
    pid: u32,
    current_session: u32,
    connection_cache: Cell<Option<(Instant, bool)>>,
}

impl TargetGuard {
    pub(crate) fn new(target: &TargetWindow) -> Result<Self, ToolFailure> {
        let hwnd = target.handle()? as HWND;
        if target.pid == 0 {
            return Err(ToolFailure::new(
                ErrorCode::InvalidArguments,
                "target PID must be nonzero",
            ));
        }
        let guard = Self {
            hwnd,
            pid: target.pid,
            current_session: session_id(unsafe { GetCurrentProcessId() })?,
            connection_cache: Cell::new(None),
        };
        guard.check()?;
        Ok(guard)
    }

    pub(crate) fn check(&self) -> Result<(), ToolFailure> {
        verify_identity(self.hwnd, self.pid)?;
        let target_session = session_id(self.pid)?;
        let started = Instant::now();
        let active = match self.connection_cache.get() {
            Some((observed, active))
                if started.duration_since(observed) < Duration::from_millis(10) =>
            {
                active
            }
            _ => {
                let active = session_active(self.current_session)?;
                // Timestamp the start of the query, never prolong its validity by
                // the amount of time the synchronous query itself took.
                self.connection_cache.set(Some((started, active)));
                active
            }
        };
        let desktop = DesktopName::query()?;
        let foreground = unsafe { GetForegroundWindow() };
        let foreground_pid = if foreground.is_null() {
            0
        } else {
            window_pid(foreground)?
        };
        check_safety(
            &SafetySnapshot {
                target_exists: unsafe { IsWindow(self.hwnd) } != 0,
                target_pid: window_pid(self.hwnd)?,
                foreground_hwnd: foreground as usize,
                foreground_pid,
                current_session: self.current_session,
                target_session,
                session_active: active,
                default_desktop: desktop.is_default(),
            },
            self.hwnd as usize,
            self.pid,
        )
    }

    pub(crate) fn resolve_point(&self, point: &PointSpec) -> Result<PointDto, ToolFailure> {
        self.check()?;
        let local = PointDto {
            x: point.x,
            y: point.y,
        };
        let resolved = match point.space {
            CoordinateSpace::Screen => local,
            CoordinateSpace::Client => {
                let client =
                    logi_vhid::client_rect(self.hwnd as isize).map_err(core_query_error)?;
                check_client_point(&local, &rect_dto(client))?;
                let converted = logi_vhid::client_to_screen(
                    self.hwnd as isize,
                    logi_vhid::Point {
                        x: point.x,
                        y: point.y,
                    },
                )
                .map_err(core_query_error)?;
                PointDto {
                    x: converted.x,
                    y: converted.y,
                }
            }
        };
        let displays = logi_vhid::monitors().map_err(core_query_error)?;
        // Test actual monitors, not their bounding box: gaps are not pixels.
        check_screen_point(
            &resolved,
            displays.iter().map(|monitor| rect_dto(monitor.rect)),
        )?;
        Ok(resolved)
    }

    pub(crate) fn hit_test(&self, point: PointDto) -> Result<(), ToolFailure> {
        self.check()?;
        let _physical = PhysicalPixels::enter()?;
        let window = unsafe {
            WindowFromPoint(POINT {
                x: point.x,
                y: point.y,
            })
        };
        if window.is_null() {
            return Err(ToolFailure::new(
                ErrorCode::PointNotOnTarget,
                "no window was found at the requested point",
            ));
        }
        let root = unsafe { GetAncestor(window, GA_ROOT) };
        if root.is_null() {
            return Err(last_failure(ErrorCode::NativeError, "GetAncestor(GA_ROOT)"));
        }
        if root != self.hwnd {
            return Err(ToolFailure::new(
                ErrorCode::PointNotOnTarget,
                "requested point is obscured or does not belong to the target root window",
            ));
        }
        // Query again after hit testing, without pretending this closes the
        // unavoidable race between a foreground check and desktop-level input.
        self.check()
    }
}

fn window_title(hwnd: HWND) -> Result<String, ToolFailure> {
    unsafe { SetLastError(0) };
    let length = unsafe { GetWindowTextLengthW(hwnd) };
    let error = unsafe { GetLastError() };
    if length == 0 && error != 0 {
        return Err(os_failure(
            ErrorCode::NativeError,
            "GetWindowTextLengthW",
            error,
        ));
    }
    let mut buffer = vec![0u16; length as usize + 1];
    unsafe { SetLastError(0) };
    let copied = unsafe { GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32) };
    let error = unsafe { GetLastError() };
    if copied == 0 && error != 0 {
        return Err(os_failure(ErrorCode::NativeError, "GetWindowTextW", error));
    }
    // Window titles are untrusted data, never executable instructions.
    Ok(String::from_utf16_lossy(&buffer[..copied as usize]))
}

fn window_info(hwnd: HWND, expected_pid: Option<u32>) -> Result<WindowDto, ToolFailure> {
    if unsafe { IsWindow(hwnd) } == 0 {
        return Err(ToolFailure::new(
            ErrorCode::TargetChanged,
            "window no longer exists",
        ));
    }
    let pid = window_pid(hwnd)?;
    if expected_pid.is_some_and(|expected| expected != pid) {
        return Err(ToolFailure::new(
            ErrorCode::TargetChanged,
            "window PID has changed",
        ));
    }
    let rect = logi_vhid::window_rect(hwnd as isize).map_err(core_query_error)?;
    let client = logi_vhid::client_rect(hwnd as isize).map_err(core_query_error)?;
    let origin = logi_vhid::client_to_screen(hwnd as isize, logi_vhid::Point { x: 0, y: 0 })
        .map_err(core_query_error)?;
    let dpi = logi_vhid::window_dpi(hwnd as isize).map_err(core_query_error)?;
    let title = window_title(hwnd)?;
    let session = session_id(pid)?;
    verify_identity(hwnd, pid)?;
    Ok(WindowDto {
        hwnd: format!("0x{:x}", hwnd as usize),
        pid,
        title,
        rect: rect_dto(rect),
        client_rect: rect_dto(client),
        client_origin: PointDto {
            x: origin.x,
            y: origin.y,
        },
        dpi,
        session_id: session,
    })
}

pub(crate) fn desktop_info(window: Option<&TargetWindow>) -> Result<DesktopInfo, ToolFailure> {
    let session = session_id(unsafe { GetCurrentProcessId() })?;
    let active = session_active(session)?;
    let desktop = DesktopName::query()?;
    let _physical = PhysicalPixels::enter()?;
    let virtual_desktop = logi_vhid::virtual_desktop_rect().map_err(core_query_error)?;
    let displays = logi_vhid::monitors().map_err(core_query_error)?;
    if displays.is_empty() {
        return Err(ToolFailure::new(
            ErrorCode::DesktopUnavailable,
            "no physical monitors are available",
        ));
    }
    let cursor = logi_vhid::cursor_pos().map_err(core_query_error)?;
    let foreground = unsafe { GetForegroundWindow() };
    let foreground = if foreground.is_null() {
        None
    } else {
        Some(window_info(foreground, None)?)
    };
    let window = window
        .map(|target| {
            if target.pid == 0 {
                return Err(ToolFailure::new(
                    ErrorCode::InvalidArguments,
                    "target PID must be nonzero",
                ));
            }
            window_info(target.handle()? as HWND, Some(target.pid))
        })
        .transpose()?;
    let observed = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|_| {
        ToolFailure::new(
            ErrorCode::NativeError,
            "system clock is before the Unix epoch",
        )
    })?;
    let observed_at_unix_ms = u64::try_from(observed.as_millis()).map_err(|_| {
        ToolFailure::new(
            ErrorCode::NativeError,
            "system clock exceeds the timestamp range",
        )
    })?;
    Ok(DesktopInfo {
        coordinate_space: "physical_pixels".to_owned(),
        virtual_desktop: rect_dto(virtual_desktop),
        monitors: displays
            .into_iter()
            .map(|monitor| MonitorDto {
                handle: format!("0x{:x}", monitor.handle as usize),
                rect: rect_dto(monitor.rect),
                work_rect: rect_dto(monitor.work_area),
                primary: monitor.primary,
            })
            .collect(),
        cursor: PointDto {
            x: cursor.x,
            y: cursor.y,
        },
        foreground,
        window,
        session_id: session,
        desktop_name: String::from_utf16_lossy(&desktop.units[..desktop.len]),
        interactive: active && desktop.is_default(),
        observed_at_unix_ms,
    })
}

// An owned Windows mutex must be acquired, released and closed on the worker
// thread. Its raw HANDLE intentionally provides no Send/Sync implementation.
pub(crate) struct InputMutex(HANDLE);

impl InputMutex {
    pub(crate) fn acquire() -> Result<(Self, bool), ToolFailure> {
        const NAME: windows_sys::core::PCWSTR =
            windows_sys::core::w!(r"Global\logi-vhid-mcp-input");
        let handle = unsafe { CreateMutexW(null(), 0, NAME) };
        if handle.is_null() {
            return Err(last_failure(ErrorCode::NativeError, "CreateMutexW"));
        }
        let wait = unsafe { WaitForSingleObject(handle, 0) };
        match wait {
            WAIT_OBJECT_0 | WAIT_ABANDONED => Ok((Self(handle), wait == WAIT_ABANDONED)),
            WAIT_TIMEOUT => {
                unsafe { CloseHandle(handle) };
                Err(ToolFailure::new(
                    ErrorCode::DeviceBusy,
                    "another cooperating input client owns the input mutex",
                ))
            }
            _ => {
                let failure = last_failure(ErrorCode::NativeError, "WaitForSingleObject");
                unsafe { CloseHandle(handle) };
                Err(failure)
            }
        }
    }
}

impl Drop for InputMutex {
    fn drop(&mut self) {
        if unsafe { ReleaseMutex(self.0) } == 0 {
            eprintln!("failed to release input mutex: OS error {}", unsafe {
                GetLastError()
            });
        }
        if unsafe { CloseHandle(self.0) } == 0 {
            eprintln!("failed to close input mutex: OS error {}", unsafe {
                GetLastError()
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn safe_snapshot() -> SafetySnapshot {
        SafetySnapshot {
            target_exists: true,
            target_pid: 7,
            foreground_hwnd: 0x123,
            foreground_pid: 7,
            current_session: 2,
            target_session: 2,
            session_active: true,
            default_desktop: true,
        }
    }

    #[test]
    fn rejects_reused_handle_and_changed_foreground() {
        let mut snapshot = safe_snapshot();
        assert!(check_safety(&snapshot, 0x123, 7).is_ok());
        snapshot.target_pid = 8;
        assert!(matches!(
            check_safety(&snapshot, 0x123, 7).unwrap_err().code,
            ErrorCode::TargetChanged
        ));
        snapshot = safe_snapshot();
        snapshot.foreground_hwnd = 0x456;
        assert!(matches!(
            check_safety(&snapshot, 0x123, 7).unwrap_err().code,
            ErrorCode::TargetChanged
        ));
        snapshot = safe_snapshot();
        snapshot.foreground_pid = 8;
        assert!(matches!(
            check_safety(&snapshot, 0x123, 7).unwrap_err().code,
            ErrorCode::TargetChanged
        ));
        snapshot = safe_snapshot();
        snapshot.target_exists = false;
        assert!(matches!(
            check_safety(&snapshot, 0x123, 7).unwrap_err().code,
            ErrorCode::TargetChanged
        ));
    }

    #[test]
    fn rejects_other_session_secure_desktop_and_disconnected_session() {
        let mut snapshot = safe_snapshot();
        snapshot.target_session = 3;
        assert!(matches!(
            check_safety(&snapshot, 0x123, 7).unwrap_err().code,
            ErrorCode::DesktopUnavailable
        ));
        snapshot = safe_snapshot();
        snapshot.default_desktop = false;
        assert!(matches!(
            check_safety(&snapshot, 0x123, 7).unwrap_err().code,
            ErrorCode::DesktopUnavailable
        ));
        snapshot = safe_snapshot();
        snapshot.session_active = false;
        assert!(matches!(
            check_safety(&snapshot, 0x123, 7).unwrap_err().code,
            ErrorCode::DesktopUnavailable
        ));
    }

    #[test]
    fn accepts_negative_monitor_points_but_rejects_gaps_and_exclusive_edges() {
        let displays = [
            RectDto {
                left: -1920,
                top: -200,
                right: 0,
                bottom: 880,
            },
            RectDto {
                left: 100,
                top: 0,
                right: 2020,
                bottom: 1080,
            },
        ];
        assert!(
            check_screen_point(&PointDto { x: -1500, y: -100 }, displays.iter().copied()).is_ok()
        );
        for point in [
            PointDto { x: 50, y: 200 },
            PointDto { x: -1, y: 880 },
            PointDto { x: 2020, y: 0 },
        ] {
            assert!(matches!(
                check_screen_point(&point, displays.iter().copied())
                    .unwrap_err()
                    .code,
                ErrorCode::InvalidArguments
            ));
        }
    }

    #[test]
    fn client_bounds_are_checked_before_conversion() {
        let client = RectDto {
            left: 0,
            top: 0,
            right: 640,
            bottom: 480,
        };
        assert!(check_client_point(&PointDto { x: 639, y: 479 }, &client).is_ok());
        for point in [
            PointDto { x: -1, y: 0 },
            PointDto { x: 640, y: 0 },
            PointDto { x: 0, y: 480 },
        ] {
            assert!(check_client_point(&point, &client).is_err());
        }
    }

    #[test]
    fn desktop_name_requires_complete_default_name_case_insensitively() {
        assert!(is_default_desktop(&[68, 101, 102, 97, 117, 108, 116]));
        assert!(is_default_desktop(&[100, 69, 70, 65, 85, 76, 84]));
        assert!(!is_default_desktop(&[68, 101, 102, 97, 117, 108, 116, 50]));
        assert!(!is_default_desktop(&[]));
    }

    #[test]
    fn zero_native_error_is_preserved_without_claiming_success() {
        let failure = os_failure(ErrorCode::NativeError, "TestQuery", 0);
        let native = failure.native.unwrap();
        assert_eq!(native.os_code, Some(0));
        assert_eq!(native.context.as_deref(), Some("TestQuery"));
        assert!(!native.message.to_lowercase().contains("success"));
    }
}
