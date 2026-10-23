//! Wire format for the Logitech virtual HID control device.
//!
//! The control device accepts custom IOCTLs with fixed-size buffers.  These
//! structs keep the byte layout in one place and make it testable.

/// Create a virtual device on the G HUB bus.
pub const IOCTL_CREATE_DEVICE: u32 = 0x2A2040;
/// Remove a virtual device from the G HUB bus.
pub const IOCTL_REMOVE_DEVICE: u32 = 0x2A2004;
/// Send a keyboard input report.
pub const IOCTL_KEYBOARD_REPORT: u32 = 0x2A200C;
/// Send a mouse input report.
pub const IOCTL_MOUSE_REPORT: u32 = 0x2A2010;
/// Register the current process id.  This disables mouse movement on the
/// virtual mouse while the process (or any registered process) is alive.
pub const IOCTL_REGISTER_PID: u32 = 0x2A2044;
/// Query the registered process id list (132-byte buffer).
pub const IOCTL_QUERY_PIDS: u32 = 0x2A2048;

/// Joystick kind.  The create-device handler rejects this value; only
/// [`DEVICE_KIND_KEYBOARD`] and [`DEVICE_KIND_MOUSE`] are accepted.  Kept for
/// protocol documentation and future research; not a supported device.
pub const DEVICE_KIND_JOYSTICK: u32 = 0x6A;
pub const DEVICE_KIND_KEYBOARD: u32 = 0x6B;
pub const DEVICE_KIND_MOUSE: u32 = 0x6D;

/// Eight-byte boot-keyboard input report: modifiers, reserved, six key slots.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct KeyboardReport {
    pub modifiers: u8,
    pub keys: [u8; 6],
}

impl KeyboardReport {
    pub fn to_bytes(self) -> [u8; 8] {
        let mut bytes = [0u8; 8];
        bytes[0] = self.modifiers;
        bytes[2..8].copy_from_slice(&self.keys);
        bytes
    }
}

/// Eight-byte relative mouse report.
///
/// Layout: buttons `u16`, dx `i16`, dy `i16`, wheel `i8`, pan `i8`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MouseReport {
    pub buttons: u16,
    pub dx: i16,
    pub dy: i16,
    pub wheel: i8,
    pub pan: i8,
}

impl MouseReport {
    pub fn to_bytes(self) -> [u8; 8] {
        let mut bytes = [0u8; 8];
        bytes[0..2].copy_from_slice(&self.buttons.to_le_bytes());
        bytes[2..4].copy_from_slice(&self.dx.to_le_bytes());
        bytes[4..6].copy_from_slice(&self.dy.to_le_bytes());
        bytes[6] = self.wheel as u8;
        bytes[7] = self.pan as u8;
        bytes
    }
}

/// Create-device request: `[8, 0, 0, 0, kind, 0, 0, 0]`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CreateDevice {
    pub kind: u32,
}

impl CreateDevice {
    pub fn to_bytes(self) -> [u8; 8] {
        debug_assert!(self.kind <= u8::MAX as u32);
        [8, 0, 0, 0, self.kind as u8, 0, 0, 0]
    }
}

/// Remove-device request: size 12, kind at offset 8.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RemoveDevice {
    pub kind: u32,
}

impl RemoveDevice {
    pub fn to_bytes(self) -> [u8; 12] {
        let mut bytes = [0u8; 12];
        bytes[0..4].copy_from_slice(&12u32.to_le_bytes());
        bytes[8..12].copy_from_slice(&self.kind.to_le_bytes());
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyboard_report_layout() {
        let report = KeyboardReport {
            modifiers: 0x02,
            keys: [0x04, 0x05, 0, 0, 0, 0],
        };
        assert_eq!(report.to_bytes(), [0x02, 0, 0x04, 0x05, 0, 0, 0, 0]);
    }

    #[test]
    fn mouse_report_layout() {
        let report = MouseReport {
            buttons: 0x0003,
            dx: 0x0102,
            dy: -2,
            wheel: 1,
            pan: -1,
        };
        assert_eq!(
            report.to_bytes(),
            [0x03, 0x00, 0x02, 0x01, 0xFE, 0xFF, 0x01, 0xFF]
        );
    }

    #[test]
    fn create_and_remove_layout() {
        assert_eq!(
            CreateDevice {
                kind: DEVICE_KIND_MOUSE
            }
            .to_bytes(),
            [8, 0, 0, 0, 0x6D, 0, 0, 0]
        );
        assert_eq!(
            RemoveDevice {
                kind: DEVICE_KIND_KEYBOARD
            }
            .to_bytes(),
            [12, 0, 0, 0, 0, 0, 0, 0, 0x6B, 0, 0, 0]
        );
    }
}
