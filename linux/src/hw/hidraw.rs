//! Control transport over Linux hidraw. Uses only the receiver interface
//! whose descriptor declares the mouse collection and a 90-byte feature
//! report (interface 0, the one OpenRazer addresses). The kernel turns
//! HIDIOCSFEATURE / HIDIOCGFEATURE into HID class SET_REPORT / GET_REPORT.
//! No interface probing, detaching or undocumented writes are attempted.

use super::protocol::{finish_response, RazerError, RazerTransport, Result, REPORT_LENGTH};
use super::sysfs::{self, HidDevice};
use crate::model::data_folder;
use std::fs::{File, OpenOptions};
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::time::Duration;

const fn ioc(dir: u32, kind: u32, nr: u32, size: u32) -> libc::c_ulong {
    ((dir << 30) | (size << 16) | (kind << 8) | nr) as libc::c_ulong
}
const IOC_READ_WRITE: u32 = 3;
pub const fn hidiocsfeature(len: u32) -> libc::c_ulong {
    ioc(IOC_READ_WRITE, b'H' as u32, 0x06, len)
}
pub const fn hidiocgfeature(len: u32) -> libc::c_ulong {
    ioc(IOC_READ_WRITE, b'H' as u32, 0x07, len)
}

/// Receiver interfaces that qualify as the control channel.
pub fn control_candidates(devices: &[HidDevice]) -> Vec<HidDevice> {
    devices
        .iter()
        .filter(|d| {
            d.bus == sysfs::BUS_USB
                && d.vendor == sysfs::RAZER_VENDOR
                && d.product == sysfs::RECEIVER_PRODUCT
                && d.descriptor.primary_usage() == Some((1, 2))
                && d.descriptor.feature_bytes(0) == Some(REPORT_LENGTH as u32)
                && d.hidraw.is_some()
        })
        .cloned()
        .collect()
}

pub struct HidrawTransport {
    file: File,
    _lock: File,
    identity: String,
    pub path: PathBuf,
}

impl HidrawTransport {
    pub fn open() -> Result<Self> {
        let candidates = control_candidates(&sysfs::naga_devices());
        if candidates.is_empty() {
            return Err(RazerError::Disconnected);
        }
        if candidates.len() > 1 {
            return Err(RazerError::Transport(
                "USB interface missing or ambiguous. Connect a single Naga V2 HyperSpeed receiver.".into(),
            ));
        }
        let device = &candidates[0];
        // Serialize full sessions across the app and command-line runs.
        let lock_path = data_folder::file("hardware.lock");
        crate::model::store::create_private_dir(&data_folder::root()).map_err(|e| RazerError::Transport(e.0))?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_CLOEXEC)
            .open(&lock_path)
            .map_err(|_| RazerError::Transport("Could not reserve communication with the mouse.".into()))?;
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err(RazerError::Transport("The mouse is busy with another OpenNaga operation. Try again.".into()));
        }
        let path = device.hidraw.clone().unwrap_or_default();
        let file = OpenOptions::new().read(true).write(true).custom_flags(libc::O_CLOEXEC).open(&path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::PermissionDenied {
                RazerError::Transport(format!(
                    "No permission to open {}. Install the OpenNaga access rule from Status, then reconnect the receiver.",
                    path.display()
                ))
            } else {
                RazerError::Transport(format!("USB open: {error}."))
            }
        })?;
        // The location stays stable across reconnects to the same port, so
        // recovery records are never applied to a receiver on another port.
        let identity = format!("1532:00b4:{}", device.location());
        Ok(HidrawTransport { file, _lock: lock, identity, path })
    }

    fn set_feature(&self, request: &[u8]) -> Result<()> {
        let mut buffer = [0u8; REPORT_LENGTH + 1];
        buffer[1..].copy_from_slice(request);
        let result = unsafe { libc::ioctl(self.file.as_raw_fd(), hidiocsfeature(buffer.len() as u32), buffer.as_mut_ptr()) };
        if result < 0 {
            return Err(io_error("USB write"));
        }
        Ok(())
    }

    fn get_feature(&self) -> Result<Vec<u8>> {
        let mut buffer = [0u8; REPORT_LENGTH + 1];
        let result = unsafe { libc::ioctl(self.file.as_raw_fd(), hidiocgfeature(buffer.len() as u32), buffer.as_mut_ptr()) };
        if result < 0 {
            return Err(io_error("USB read"));
        }
        // This interface has no numbered reports. The kernel keeps byte 0 for
        // the report number and counts it in the result.
        let length = result as usize;
        if length == REPORT_LENGTH + 1 {
            Ok(buffer[1..].to_vec())
        } else {
            Err(RazerError::Malformed(format!("Incomplete USB report: {length} bytes.")))
        }
    }
}

fn io_error(operation: &str) -> RazerError {
    let error = std::io::Error::last_os_error();
    match error.raw_os_error() {
        Some(libc::ENODEV) | Some(libc::ENOENT) => RazerError::Disconnected,
        Some(libc::ETIMEDOUT) | Some(libc::EPIPE) => RazerError::Transport("USB timeout. Reconnect the receiver.".into()),
        _ => RazerError::Transport(format!("{operation}: {error}.")),
    }
}

impl RazerTransport for HidrawTransport {
    fn identity(&self) -> String {
        self.identity.clone()
    }

    fn exchange(&mut self, request: &[u8]) -> Result<Vec<u8>> {
        if request.len() != REPORT_LENGTH {
            return Err(RazerError::InvalidValue("Invalid USB report.".into()));
        }
        self.set_feature(request)?;
        // Receiver-specific response wait in OpenRazer is 31 ms.
        std::thread::sleep(Duration::from_millis(35));
        let response = self.get_feature()?;
        let this = &*self;
        finish_response(response, request, &mut std::thread::sleep, &mut || this.get_feature())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ioctl_numbers_match_the_kernel_headers() {
        // HIDIOCSFEATURE(91) and HIDIOCGFEATURE(91) from linux/hidraw.h.
        assert_eq!(hidiocsfeature(91), 0xC05B4806);
        assert_eq!(hidiocgfeature(91), 0xC05B4807);
    }
}
