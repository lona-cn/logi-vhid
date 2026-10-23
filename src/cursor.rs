//! Position the cursor through relative virtual mouse movements, Win32
//! `SendInput`, or `SetCursorPos`.

use std::thread::sleep;
use std::time::Duration;

use crate::device::Device;
use crate::error::{Error, Result};
use crate::ffi::{
    virtual_screen, with_physical_coordinates, GetAsyncKeyState, GetCursorPos, Input, MouseInput,
    Point, SendInput, SetCursorPos, INPUT_MOUSE, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_MOVE,
    MOUSEEVENTF_VIRTUALDESK,
};
use crate::mouse::MouseState;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AbsoluteMode {
    /// Read the cursor position and send relative virtual mouse movements
    /// toward the target, with a limit on attempts. Returns an error if a
    /// process registered through IOCTL `0x2A2044` has disabled mouse movement
    /// or the cursor cannot reach the target.
    Driver,
    /// Send absolute mouse coordinates through Win32 `SendInput` with
    /// `MOUSEEVENTF_ABSOLUTE`, without using the Logitech driver.
    Win32,
    /// Call Win32 `SetCursorPos` directly instead of injecting an absolute
    /// mouse report with `SendInput`.
    SetCursorPos,
}

/// Read the absolute cursor position in physical pixels.
pub fn cursor_pos() -> Result<Point> {
    with_physical_coordinates(|| {
        let mut point = Point::default();
        if unsafe { GetCursorPos(&mut point) } == 0 {
            return Err(Error::last_io("GetCursorPos"));
        }
        Ok(point)
    })
}

/// Query whether a Win32 virtual key is currently down.
pub fn is_key_down(vk: i32) -> bool {
    unsafe { (GetAsyncKeyState(vk) & 0x8000u16 as i16) != 0 }
}

/// Set the absolute cursor position with Win32 `SendInput`.
pub fn set_cursor_pos_win32(x: i32, y: i32) -> Result<()> {
    with_physical_coordinates(|| {
        let (origin_x, origin_y, width, height) = virtual_screen()?;
        let normalized_x = normalize(x, origin_x, width);
        let normalized_y = normalize(y, origin_y, height);

        let input = Input {
            type_: INPUT_MOUSE,
            mi: MouseInput {
                dx: normalized_x,
                dy: normalized_y,
                mouse_data: 0,
                flags: MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
                time: 0,
                extra_info: 0,
            },
        };
        let sent = unsafe {
            SendInput(
                1,
                &input as *const Input as *const u8,
                std::mem::size_of::<Input>() as i32,
            )
        };
        if sent != 1 {
            return Err(Error::last_io("SendInput"));
        }
        Ok(())
    })
}

/// Set the absolute cursor position with Win32 `SetCursorPos`.
///
/// Moves the desktop cursor directly rather than injecting an absolute mouse
/// report with `SendInput`.
pub fn set_cursor_pos_direct(x: i32, y: i32) -> Result<()> {
    with_physical_coordinates(|| {
        let (origin_x, origin_y, width, height) = virtual_screen()?;
        let target_x = x.clamp(origin_x, origin_x + width - 1);
        let target_y = y.clamp(origin_y, origin_y + height - 1);
        if unsafe { SetCursorPos(target_x, target_y) } == 0 {
            return Err(Error::last_io("SetCursorPos"));
        }
        Ok(())
    })
}

pub(crate) fn set_cursor_pos_driver(
    device: &Device,
    mouse: &mut MouseState,
    x: i32,
    y: i32,
) -> Result<()> {
    with_physical_coordinates(|| {
        let (origin_x, origin_y, width, height) = virtual_screen()?;
        let target_x = x.clamp(origin_x, origin_x + width - 1);
        let target_y = y.clamp(origin_y, origin_y + height - 1);

        let start = cursor_pos()?;
        if start.x != target_x || start.y != target_y {
            let mut moved = false;
            // A 1-count report can be swallowed by the pointer dead zone; probe
            // with 16 counts. The loop below re-targets the absolute position.
            mouse.move_relative(device, 16, 0)?;
            sleep(Duration::from_millis(30));
            let probed = cursor_pos()?;
            if probed.x != start.x || probed.y != start.y {
                moved = true;
            }
            if !moved {
                // The cursor may be at the right edge; try the other direction.
                mouse.move_relative(device, -16, 0)?;
                sleep(Duration::from_millis(30));
                let probed = cursor_pos()?;
                if probed.x != start.x || probed.y != start.y {
                    moved = true;
                }
            }
            if !moved {
                let pids = device.registered_pids().unwrap_or_default();
                let detail = if pids.is_empty() {
                    "no PIDs registered; the virtual mouse device or report path may be inactive"
                        .to_string()
                } else {
                    format!("registered PIDs: {pids:?}")
                };
                return Err(Error::Protocol(format!(
                    "virtual mouse did not move at all; IOCTL 0x2A2044 may have disabled it ({detail})"
                )));
            }
        }

        let mut last = (i32::MIN, i32::MIN);
        let mut stalled = 0;
        // Windows pointer acceleration amplifies a large relative report; sending
        // the whole remaining delta at once can overshoot to a screen edge and
        // stall the correction. At most 64 counts per report converges.
        for _ in 0..200 {
            let current = cursor_pos()?;
            let dx = i64::from(target_x) - i64::from(current.x);
            let dy = i64::from(target_y) - i64::from(current.y);
            if dx == 0 && dy == 0 {
                return Ok(());
            }
            if (current.x, current.y) == last {
                stalled += 1;
            } else {
                stalled = 0;
            }
            if stalled >= 10 {
                return Err(Error::Protocol(format!(
                    "driver mouse stopped moving at ({},{}), target ({target_x},{target_y})",
                    current.x, current.y
                )));
            }
            last = (current.x, current.y);
            let step_x = dx.clamp(-64, 64) as i32;
            let step_y = dy.clamp(-64, 64) as i32;
            mouse.move_relative(device, step_x, step_y)?;
            sleep(Duration::from_millis(10));
        }

        let current = cursor_pos()?;
        if current.x == target_x && current.y == target_y {
            Ok(())
        } else {
            Err(Error::Protocol(format!(
                "could not reach ({target_x},{target_y}); cursor is at ({},{})",
                current.x, current.y
            )))
        }
    })
}

fn normalize(value: i32, origin: i32, size: i32) -> i32 {
    debug_assert!(size > 0);
    let size = i64::from(size);
    let offset = (i64::from(value) - i64::from(origin)).clamp(0, size - 1);
    // Round up to the first absolute unit inside the requested pixel's interval.
    ((offset * 65536 + size - 1) / size).clamp(0, 65535) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_coordinates_map_back_to_the_requested_pixel() {
        // SendInput maps the 65536 absolute units to `size` pixel intervals,
        // not to `size - 1` gaps between endpoints.
        for (origin, size) in [
            (0, 1),
            (0, 2),
            (0, 1920),
            (-1920, 3840),
            (-7680, 15360),
            (-19200, 38400),
            (-32768, 65536),
            (i32::MIN, 1920),
            (i32::MAX - 1919, 1920),
        ] {
            for offset in 0..size {
                let pixel = origin + offset;
                let absolute = normalize(pixel, origin, size);
                assert!((0..=65535).contains(&absolute));
                let mapped = i64::from(origin) + i64::from(absolute) * i64::from(size) / 65536;
                assert_eq!(mapped, i64::from(pixel), "origin={origin}, size={size}");
            }
        }
    }

    #[test]
    fn normalized_outside_coordinates_map_to_boundary_pixels() {
        for (origin, size) in [(-1920, 3840), (0, 1), (0, 1920)] {
            for (value, expected) in [(i32::MIN, origin), (i32::MAX, origin + size - 1)] {
                let absolute = normalize(value, origin, size);
                let mapped = i64::from(origin) + i64::from(absolute) * i64::from(size) / 65536;
                assert_eq!(mapped, i64::from(expected));
            }
        }
    }
}
