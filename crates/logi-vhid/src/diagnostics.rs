//! Device paths and processes registered with the driver.
//!
//! Registering a process disables virtual mouse movement. These helpers report
//! that state; they do not register processes or re-enable mouse movement.

use crate::device::find_hid_path;
use crate::error::{Error, Result};
use crate::LogitechVirtualHid;

/// A snapshot of device paths and registered process IDs.
///
/// Child paths are the first present HID interfaces matching the Logitech
/// keyboard/mouse VID/PID system-wide, not verified descendants of `control_path`.
/// They may be present even if that device was not selected in this object's
/// [`crate::OpenOptions`]. A path's presence does not prove report delivery works.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceInfo {
    pub control_path: String,
    pub keyboard_path: Option<String>,
    pub mouse_path: Option<String>,
    pub registered_pids: Vec<u64>,
}

impl DeviceInfo {
    /// Whether this snapshot has any registered PIDs, which gate relative mouse
    /// X/Y in the driver. Does not query current state or cover button/wheel input.
    pub fn pid_gate_active(&self) -> bool {
        !self.registered_pids.is_empty()
    }
}

impl LogitechVirtualHid {
    /// Query the control path, system-wide matching HID paths and registered PIDs.
    /// This does not create devices, change PID registration or test input.
    pub fn device_info(&self) -> Result<DeviceInfo> {
        Ok(DeviceInfo {
            control_path: self.device.path().to_string(),
            keyboard_path: find_hid_path(0x046D, 0xC232)?,
            mouse_path: find_hid_path(0x046D, 0xC231)?,
            registered_pids: self.device.registered_pids()?,
        })
    }

    /// Whether any process has registered a PID with IOCTL `0x2A2044`.
    pub fn is_pid_gate_active(&self) -> Result<bool> {
        Ok(!self.device.registered_pids()?.is_empty())
    }

    /// Check only whether the current registered-PID list is empty.
    ///
    /// A non-empty list returns [`Error::Protocol`]. Success does not verify that
    /// a mouse was opened, its child exists, or movement works, and the list can
    /// change after this query. This check does not change the driver's state.
    pub fn ensure_mouse_available(&self) -> Result<()> {
        let pids = self.device.registered_pids()?;
        if pids.is_empty() {
            Ok(())
        } else {
            Err(Error::Protocol(format!(
                "virtual mouse is disabled by registered PID(s): {pids:?}"
            )))
        }
    }
}
