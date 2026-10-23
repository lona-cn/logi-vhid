//! Device paths and processes registered with the driver.
//!
//! Registering a process disables virtual mouse movement. These helpers report
//! that state; they do not register processes or re-enable mouse movement.

use crate::device::find_hid_path;
use crate::error::{Error, Result};
use crate::LogitechVirtualHid;

/// Device paths and registered process IDs for the open virtual HID bus.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceInfo {
    pub control_path: String,
    pub keyboard_path: Option<String>,
    pub mouse_path: Option<String>,
    pub registered_pids: Vec<u64>,
}

impl DeviceInfo {
    /// A non-empty PID list means the driver zeroes mouse X/Y.
    pub fn pid_gate_active(&self) -> bool {
        !self.registered_pids.is_empty()
    }
}

impl LogitechVirtualHid {
    /// Read the control path, child HID paths, and registered PID list.
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

    /// Return an error if registered processes have disabled mouse movement.
    /// This check does not change the driver's state.
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
