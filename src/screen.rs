//! Monitor, window, and client-coordinate helpers.

use std::ptr::null;

use crate::error::{Error, Result};
use crate::ffi::{
    virtual_screen, with_physical_coordinates, ClientToScreen, EnumDisplayMonitors, EnumWindows,
    GetClientRect, GetDpiForWindow, GetForegroundWindow, GetMonitorInfoW, GetWindowRect,
    GetWindowTextLengthW, GetWindowTextW, IsWindowVisible, MonitorFromPoint, MonitorInfoRaw, Point,
    Rect, ScreenToClient, BOOL, MONITORINFOF_PRIMARY, MONITOR_DEFAULTTONEAREST,
};

/// Monitor rectangle and metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MonitorInfo {
    pub handle: isize,
    pub rect: Rect,
    pub work_area: Rect,
    pub primary: bool,
}

/// Bounding rectangle of the whole virtual desktop, in physical pixels.
pub fn virtual_desktop_rect() -> Result<Rect> {
    with_physical_coordinates(|| {
        let (x, y, width, height) = virtual_screen()?;
        Ok(Rect {
            left: x,
            top: y,
            right: x + width,
            bottom: y + height,
        })
    })
}

/// Enumerate all monitors in the current desktop.
pub fn monitors() -> Result<Vec<MonitorInfo>> {
    with_physical_coordinates(|| unsafe {
        let mut handles: Vec<isize> = Vec::new();
        let ok = EnumDisplayMonitors(
            0,
            null(),
            Some(collect_monitor),
            &mut handles as *mut Vec<isize> as isize,
        );
        if ok == 0 {
            return Err(Error::last_io("EnumDisplayMonitors"));
        }

        let mut result = Vec::with_capacity(handles.len());
        for handle in handles {
            result.push(monitor_info(handle)?);
        }
        Ok(result)
    })
}

/// Monitor containing or nearest to a point.
pub fn monitor_from_point(point: Point) -> Result<MonitorInfo> {
    with_physical_coordinates(|| unsafe {
        let handle = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
        if handle == 0 {
            return Err(Error::last_io("MonitorFromPoint"));
        }
        monitor_info(handle)
    })
}

unsafe extern "system" fn collect_monitor(
    monitor: isize,
    _hdc: isize,
    _rect: *mut Rect,
    data: isize,
) -> BOOL {
    let handles = &mut *(data as *mut Vec<isize>);
    handles.push(monitor);
    1
}

unsafe fn monitor_info(handle: isize) -> Result<MonitorInfo> {
    let mut raw = MonitorInfoRaw {
        cb_size: std::mem::size_of::<MonitorInfoRaw>() as u32,
        ..Default::default()
    };
    if GetMonitorInfoW(handle, &mut raw) == 0 {
        return Err(Error::last_io("GetMonitorInfoW"));
    }
    Ok(MonitorInfo {
        handle,
        rect: raw.monitor,
        work_area: raw.work,
        primary: raw.flags & MONITORINFOF_PRIMARY != 0,
    })
}

/// Foreground window handle, if any.
pub fn foreground_window() -> Option<isize> {
    let window = unsafe { GetForegroundWindow() };
    if window == 0 {
        None
    } else {
        Some(window)
    }
}

/// Find the first visible window whose title contains `needle` (case-insensitive).
pub fn find_window_by_title(needle: &str) -> Result<isize> {
    if needle.is_empty() {
        return Err(Error::Unsupported(
            "window title search needs a non-empty substring".to_string(),
        ));
    }

    struct Search {
        needle: String,
        found: isize,
    }

    unsafe extern "system" fn callback(window: isize, data: isize) -> BOOL {
        let search = &mut *(data as *mut Search);
        if IsWindowVisible(window) == 0 {
            return 1;
        }
        let length = GetWindowTextLengthW(window);
        if length <= 0 {
            return 1;
        }
        let mut title = vec![0u16; length as usize + 1];
        let copied = GetWindowTextW(window, title.as_mut_ptr(), title.len() as i32);
        if copied <= 0 {
            return 1;
        }
        let title = String::from_utf16_lossy(&title[..copied as usize]);
        if title.to_lowercase().contains(&search.needle) {
            search.found = window;
            return 0;
        }
        1
    }

    let mut search = Search {
        needle: needle.to_lowercase(),
        found: 0,
    };
    unsafe {
        EnumWindows(Some(callback), &mut search as *mut Search as isize);
    }
    if search.found == 0 {
        Err(Error::NotFound(format!(
            "no visible window title contains {needle:?}"
        )))
    } else {
        Ok(search.found)
    }
}

/// Window rectangle in physical screen pixels.
pub fn window_rect(window: isize) -> Result<Rect> {
    with_physical_coordinates(|| {
        let mut rect = Rect::default();
        if unsafe { GetWindowRect(window, &mut rect) } == 0 {
            return Err(Error::last_io("GetWindowRect"));
        }
        Ok(rect)
    })
}

/// Client rectangle in physical, window-local pixels.
pub fn client_rect(window: isize) -> Result<Rect> {
    with_physical_coordinates(|| {
        let mut rect = Rect::default();
        if unsafe { GetClientRect(window, &mut rect) } == 0 {
            return Err(Error::last_io("GetClientRect"));
        }
        Ok(rect)
    })
}

/// Convert a physical client-relative point to physical screen coordinates.
pub fn client_to_screen(window: isize, point: Point) -> Result<Point> {
    with_physical_coordinates(|| {
        let mut point = point;
        if unsafe { ClientToScreen(window, &mut point) } == 0 {
            return Err(Error::last_io("ClientToScreen"));
        }
        Ok(point)
    })
}

/// Convert a physical screen point to physical client-relative coordinates.
pub fn screen_to_client(window: isize, point: Point) -> Result<Point> {
    with_physical_coordinates(|| {
        let mut point = point;
        if unsafe { ScreenToClient(window, &mut point) } == 0 {
            return Err(Error::last_io("ScreenToClient"));
        }
        Ok(point)
    })
}

/// DPI of a window (96 = 100% scaling).
pub fn window_dpi(window: isize) -> Result<u32> {
    with_physical_coordinates(|| {
        let dpi = unsafe { GetDpiForWindow(window) };
        if dpi == 0 {
            return Err(Error::last_io("GetDpiForWindow"));
        }
        Ok(dpi)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_geometry() {
        let rect = Rect {
            left: 10,
            top: 20,
            right: 110,
            bottom: 70,
        };
        assert_eq!(rect.width(), 100);
        assert_eq!(rect.height(), 50);
        assert!(rect.contains(Point { x: 10, y: 20 }));
        assert!(rect.contains(Point { x: 109, y: 69 }));
        assert!(!rect.contains(Point { x: 110, y: 69 }));
        assert!(!rect.contains(Point { x: 10, y: 70 }));
    }

    #[link(name = "user32")]
    extern "system" {
        fn SetThreadDpiAwarenessContext(value: isize) -> isize;
        fn GetThreadDpiAwarenessContext() -> isize;
        fn AreDpiAwarenessContextsEqual(first: isize, second: isize) -> BOOL;
        fn GetDesktopWindow() -> isize;
    }

    struct TestDpiContext(isize);

    impl TestDpiContext {
        fn enter(context: isize) -> Self {
            let previous = unsafe { SetThreadDpiAwarenessContext(context) };
            assert_ne!(previous, 0, "set test thread DPI context");
            Self(previous)
        }
    }

    impl Drop for TestDpiContext {
        fn drop(&mut self) {
            unsafe { SetThreadDpiAwarenessContext(self.0) };
        }
    }

    fn in_context<T>(context: isize, operation: impl FnOnce() -> T) -> T {
        let _restore = TestDpiContext::enter(context);
        let result = operation();
        assert_ne!(
            unsafe { AreDpiAwarenessContextsEqual(GetThreadDpiAwarenessContext(), context) },
            0,
            "coordinate query must restore its caller's thread DPI context"
        );
        result
    }

    #[derive(Debug, PartialEq)]
    struct Coordinates {
        desktop: Rect,
        monitors: Vec<MonitorInfo>,
        nearest: MonitorInfo,
        window: Rect,
        client: Rect,
        client_origin: Point,
        client_point: Point,
        dpi: u32,
    }

    fn coordinates(context: isize, window: isize) -> Coordinates {
        let desktop = in_context(context, || virtual_desktop_rect().expect("virtual desktop"));
        let mut monitors = in_context(context, || monitors().expect("enumerate monitors"));
        monitors.sort_by_key(|monitor| monitor.handle);
        let point = Point {
            x: desktop.left,
            y: desktop.top,
        };
        let nearest = in_context(context, || {
            monitor_from_point(point).expect("nearest monitor")
        });
        let client_origin = in_context(context, || {
            client_to_screen(window, Point::default()).expect("client origin")
        });
        let client_point = in_context(context, || {
            screen_to_client(window, client_origin).expect("screen to client")
        });
        assert_eq!(client_point, Point::default());
        Coordinates {
            desktop,
            monitors,
            nearest,
            window: in_context(context, || window_rect(window).expect("window rectangle")),
            client: in_context(context, || client_rect(window).expect("client rectangle")),
            client_origin,
            client_point,
            dpi: in_context(context, || window_dpi(window).expect("window DPI")),
        }
    }

    #[test]
    fn physical_queries_are_independent_of_the_callers_dpi_context_and_call_order() {
        // Use an existing system window: no input, window creation, or process
        // DPI changes. The guard also restores context if an assertion panics.
        let window = unsafe { GetDesktopWindow() };
        assert_ne!(window, 0);
        let unaware = coordinates(-1, window);
        let cursor = in_context(-1, || crate::cursor::cursor_pos().expect("cursor position"));
        assert!(unaware.desktop.contains(cursor));
        let after_cursor_query = coordinates(-1, window);
        let per_monitor = coordinates(-4, window);
        assert_eq!(unaware, after_cursor_query);
        assert_eq!(unaware, per_monitor);
        assert_eq!(unaware, coordinates(-3, window));
        let bounds = unaware
            .monitors
            .iter()
            .fold(unaware.monitors[0].rect, |bounds, monitor| Rect {
                left: bounds.left.min(monitor.rect.left),
                top: bounds.top.min(monitor.rect.top),
                right: bounds.right.max(monitor.rect.right),
                bottom: bounds.bottom.max(monitor.rect.bottom),
            });
        assert_eq!(unaware.desktop, bounds);
    }

    #[test]
    fn failed_coordinate_queries_restore_the_callers_context() {
        for context in [-1, -3, -4] {
            assert!(in_context(context, || window_rect(0)).is_err());
            assert!(in_context(context, || client_rect(0)).is_err());
            assert!(in_context(context, || client_to_screen(0, Point::default())).is_err());
            assert!(in_context(context, || screen_to_client(0, Point::default())).is_err());
            assert!(in_context(context, || window_dpi(0)).is_err());
        }
    }

    #[test]
    fn coordinate_scope_restores_context_during_unwinding() {
        in_context(-1, || {
            let panicked = std::panic::catch_unwind(|| {
                let _: Result<()> = with_physical_coordinates(|| {
                    panic!("unwind through a physical-coordinate query")
                });
            });
            assert!(panicked.is_err());
        });
    }
}
