//! Device enumeration and the control-device handle.

use std::ffi::OsStr;
use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::ptr::{null, null_mut};

use crate::error::{Error, Result};
use crate::ffi::{
    CloseHandle, CreateFileW, DeviceIoControl, Guid, SetupDiDestroyDeviceInfoList,
    SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW, SetupDiGetDeviceInterfaceDetailW,
    SpDeviceInterfaceData, DIGCF_DEVICEINTERFACE, DIGCF_PRESENT, DWORD, FILE_SHARE_READ,
    FILE_SHARE_WRITE, GENERIC_READ, GENERIC_WRITE, HANDLE, HID_GUID, INVALID_HANDLE_VALUE,
    OPEN_EXISTING, VIRTUAL_BUS_GUID,
};
use crate::protocol::{
    CreateDevice, RemoveDevice, IOCTL_CREATE_DEVICE, IOCTL_QUERY_PIDS, IOCTL_REMOVE_DEVICE,
};

/// Synchronous eight-byte HID report transport, statically dispatched.
pub(crate) trait ReportSink {
    fn send_report(&self, code: u32, report: &[u8; 8]) -> Result<()>;
}

/// An open handle to the G HUB virtual bus control device.
pub(crate) struct Device {
    handle: HANDLE,
    path: String,
}

impl Device {
    pub(crate) fn open() -> Result<Self> {
        let path = find_interface_path()?;
        let wide: Vec<u16> = OsStr::new(&path)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let handle = unsafe {
            CreateFileW(
                wide.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                null(),
                OPEN_EXISTING,
                0,
                0,
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(Error::last_io("CreateFileW"));
        }

        Ok(Self { handle, path })
    }

    pub(crate) fn path(&self) -> &str {
        &self.path
    }

    pub(crate) fn ioctl(&self, code: DWORD, input: &[u8], output: &mut [u8]) -> Result<usize> {
        let mut returned: DWORD = 0;
        let ok = unsafe {
            DeviceIoControl(
                self.handle,
                code,
                input.as_ptr(),
                input.len() as DWORD,
                output.as_mut_ptr(),
                output.len() as DWORD,
                &mut returned,
                null_mut(),
            )
        };
        if ok == 0 {
            return Err(Error::last_io("DeviceIoControl"));
        }
        Ok(returned as usize)
    }

    /// Create one virtual device.  `ERROR_INVALID_PARAMETER` is treated as
    /// "already exists" so `open()` is idempotent.
    pub(crate) fn create_device(&self, kind: u32) -> Result<()> {
        let input = CreateDevice { kind }.to_bytes();
        let mut output = [0u8; 8];
        match self.ioctl(IOCTL_CREATE_DEVICE, &input, &mut output) {
            Ok(_) => Ok(()),
            Err(err) if err.is_invalid_parameter() => Ok(()),
            Err(err) => Err(err),
        }
    }

    /// Remove one virtual device.  A missing device is treated as success.
    pub(crate) fn remove_device(&self, kind: u32) -> Result<()> {
        let input = RemoveDevice { kind }.to_bytes();
        let mut output = [0u8; 8];
        match self.ioctl(IOCTL_REMOVE_DEVICE, &input, &mut output) {
            Ok(_) => Ok(()),
            Err(err) if err.is_invalid_parameter() => Ok(()),
            Err(err) => Err(err),
        }
    }

    /// PIDs currently registered with IOCTL `0x2A2044`.
    pub(crate) fn registered_pids(&self) -> Result<Vec<u64>> {
        let mut input = [0u8; 0x84];
        input[0..4].copy_from_slice(&0x84u32.to_le_bytes());
        let mut output = [0u8; 0x84];
        self.ioctl(IOCTL_QUERY_PIDS, &input, &mut output)?;

        let mut pids = Vec::new();
        for index in 0..16 {
            let offset = 4 + index * 8;
            let pid = u64::from_le_bytes(output[offset..offset + 8].try_into().unwrap());
            if pid != 0 {
                pids.push(pid);
            }
        }
        Ok(pids)
    }
}

impl ReportSink for Device {
    fn send_report(&self, code: u32, report: &[u8; 8]) -> Result<()> {
        let mut output = [0u8; 8];
        self.ioctl(code, report, &mut output)?;
        Ok(())
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.handle);
        }
    }
}

fn interface_paths(guid: &Guid) -> Result<Vec<String>> {
    unsafe {
        let set = SetupDiGetClassDevsW(guid, null(), 0, DIGCF_PRESENT | DIGCF_DEVICEINTERFACE);
        if set == INVALID_HANDLE_VALUE {
            return Err(Error::last_io("SetupDiGetClassDevsW"));
        }

        let mut index: DWORD = 0;
        let mut paths = Vec::new();
        let result = loop {
            let mut interface = SpDeviceInterfaceData {
                cb_size: size_of::<SpDeviceInterfaceData>() as DWORD,
                interface_class_guid: *guid,
                flags: 0,
                reserved: 0,
            };

            if SetupDiEnumDeviceInterfaces(set, null(), guid, index, &mut interface) == 0 {
                let err = Error::last_io("SetupDiEnumDeviceInterfaces");
                break if err.is_no_more_items() {
                    Ok(paths)
                } else {
                    Err(err)
                };
            }

            let mut needed: DWORD = 0;
            SetupDiGetDeviceInterfaceDetailW(
                set,
                &interface,
                null_mut(),
                0,
                &mut needed,
                null_mut(),
            );
            if needed == 0 {
                index += 1;
                continue;
            }

            let mut buffer = vec![0u8; needed as usize];
            let cb_size: DWORD = if cfg!(target_pointer_width = "64") {
                8
            } else {
                6
            };
            std::ptr::write_unaligned(buffer.as_mut_ptr() as *mut DWORD, cb_size);

            if SetupDiGetDeviceInterfaceDetailW(
                set,
                &interface,
                buffer.as_mut_ptr(),
                needed,
                &mut needed,
                null_mut(),
            ) != 0
            {
                let mut path = Vec::new();
                let mut offset = 4usize;
                while offset + 2 <= buffer.len() {
                    let ch = u16::from_le_bytes([buffer[offset], buffer[offset + 1]]);
                    if ch == 0 {
                        break;
                    }
                    path.push(ch);
                    offset += 2;
                }
                paths.push(String::from_utf16_lossy(&path));
            }

            index += 1;
        };

        SetupDiDestroyDeviceInfoList(set);
        result
    }
}

fn find_interface_path() -> Result<String> {
    interface_paths(&VIRTUAL_BUS_GUID)?
        .into_iter()
        .next()
        .ok_or_else(|| {
            Error::NotFound(
                "Logitech G HUB virtual bus interface not found; is G HUB installed and logi_joy_bus_enum running?"
                    .to_string(),
            )
        })
}

/// Find the HID child interface for a vendor/product id.
pub(crate) fn find_hid_path(vendor_id: u16, product_id: u16) -> Result<Option<String>> {
    let needle = format!("vid_{vendor_id:04x}&pid_{product_id:04x}");
    Ok(interface_paths(&HID_GUID)?
        .into_iter()
        .find(|path| path.to_ascii_lowercase().contains(&needle)))
}

/// Wait for a newly created child HID device to appear in PnP.
pub(crate) fn wait_for_hid_device(
    vendor_id: u16,
    product_id: u16,
    timeout: std::time::Duration,
) -> Result<String> {
    let start = std::time::Instant::now();
    loop {
        if let Some(path) = find_hid_path(vendor_id, product_id)? {
            return Ok(path);
        }
        if start.elapsed() >= timeout {
            return Err(Error::NotFound(format!(
                "virtual HID device {:04x}:{:04x} did not appear within {:?}",
                vendor_id, product_id, timeout
            )));
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}
