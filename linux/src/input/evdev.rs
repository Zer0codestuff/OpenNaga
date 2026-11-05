//! Thin evdev and uinput wrappers over ioctls (linux/input.h, linux/uinput.h).

use crate::model::keys::ev;
use std::fs::{File, OpenOptions};
use std::io;
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

const fn ioc(dir: u32, kind: u8, nr: u32, size: u32) -> libc::c_ulong {
    ((dir << 30) | (size << 16) | ((kind as u32) << 8) | nr) as libc::c_ulong
}
const NONE: u32 = 0;
const WRITE: u32 = 1;
const READ: u32 = 2;

pub const EVIOCGRAB: libc::c_ulong = ioc(WRITE, b'E', 0x90, 4);
pub const fn eviocgbit(ev: u32, len: u32) -> libc::c_ulong {
    ioc(READ, b'E', 0x20 + ev, len)
}
pub const fn eviocgkey(len: u32) -> libc::c_ulong {
    ioc(READ, b'E', 0x18, len)
}
pub const fn eviocgname(len: u32) -> libc::c_ulong {
    ioc(READ, b'E', 0x06, len)
}

pub const UI_SET_EVBIT: libc::c_ulong = ioc(WRITE, b'U', 100, 4);
pub const UI_SET_KEYBIT: libc::c_ulong = ioc(WRITE, b'U', 101, 4);
pub const UI_SET_RELBIT: libc::c_ulong = ioc(WRITE, b'U', 102, 4);
pub const UI_DEV_SETUP: libc::c_ulong = ioc(WRITE, b'U', 3, 92);
pub const UI_DEV_CREATE: libc::c_ulong = ioc(NONE, b'U', 1, 0);
pub const UI_DEV_DESTROY: libc::c_ulong = ioc(NONE, b'U', 2, 0);

pub const KEY_BYTES: usize = (ev::KEY_MAX as usize + 1).div_ceil(8);

pub type InputEvent = libc::input_event;

pub fn event(kind: u16, code: u16, value: i32) -> InputEvent {
    InputEvent { time: libc::timeval { tv_sec: 0, tv_usec: 0 }, type_: kind, code, value }
}

fn bit(bits: &[u8], index: usize) -> bool {
    bits.get(index / 8).map(|b| b & (1 << (index % 8)) != 0).unwrap_or(false)
}

/// Capabilities of an input device, read once when it is opened.
#[derive(Clone, Debug, Default)]
pub struct Capabilities {
    pub keys: Vec<u8>,
    pub rel: Vec<u8>,
}

impl Capabilities {
    pub fn has_key(&self, code: u16) -> bool {
        bit(&self.keys, code as usize)
    }
    pub fn has_rel(&self, code: u16) -> bool {
        bit(&self.rel, code as usize)
    }
    /// Has ordinary keyboard keys (the side grid sends 1 to 0, minus, equals).
    pub fn is_keyboard(&self) -> bool {
        (ev::KEY_ESC..=ev::KEY_EQUAL).any(|c| self.has_key(c)) || self.has_key(30)
    }
    pub fn is_pointer(&self) -> bool {
        self.has_key(ev::BTN_LEFT) && self.has_rel(ev::REL_X)
    }
}

pub struct EvdevDevice {
    pub file: File,
    pub path: PathBuf,
    pub name: String,
    pub caps: Capabilities,
}

impl EvdevDevice {
    pub fn open(path: &Path) -> io::Result<Self> {
        let file = OpenOptions::new().read(true).custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC).open(path)?;
        let fd = file.as_raw_fd();
        let mut name = [0u8; 256];
        unsafe { libc::ioctl(fd, eviocgname(name.len() as u32), name.as_mut_ptr()) };
        let end = name.iter().position(|b| *b == 0).unwrap_or(name.len());
        let mut caps = Capabilities { keys: vec![0; KEY_BYTES], rel: vec![0; 2] };
        unsafe {
            libc::ioctl(fd, eviocgbit(ev::EV_KEY as u32, caps.keys.len() as u32), caps.keys.as_mut_ptr());
            libc::ioctl(fd, eviocgbit(ev::EV_REL as u32, caps.rel.len() as u32), caps.rel.as_mut_ptr());
        }
        Ok(EvdevDevice { file, path: path.to_path_buf(), name: String::from_utf8_lossy(&name[..end]).to_string(), caps })
    }

    pub fn fd(&self) -> RawFd {
        self.file.as_raw_fd()
    }

    pub fn grab(&self, on: bool) -> io::Result<()> {
        let value: libc::c_int = if on { 1 } else { 0 };
        if unsafe { libc::ioctl(self.fd(), EVIOCGRAB, value as libc::c_ulong) } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// Keys currently held, from the kernel's own state.
    pub fn held_keys(&self) -> Vec<u8> {
        held_keys(self.fd())
    }

    /// Reads every pending event without blocking.
    pub fn read_events(&self, into: &mut Vec<InputEvent>) -> io::Result<()> {
        const BATCH: usize = 64;
        let size = std::mem::size_of::<InputEvent>();
        let mut buffer: [InputEvent; BATCH] = unsafe { std::mem::zeroed() };
        loop {
            let count = unsafe { libc::read(self.fd(), buffer.as_mut_ptr() as *mut libc::c_void, size * BATCH) };
            if count < 0 {
                let error = io::Error::last_os_error();
                return match error.raw_os_error() {
                    Some(libc::EAGAIN) => Ok(()),
                    Some(libc::EINTR) => continue,
                    _ => Err(error),
                };
            }
            if count == 0 {
                return Err(io::Error::from_raw_os_error(libc::ENODEV));
            }
            into.extend_from_slice(&buffer[..count as usize / size]);
            if (count as usize) < size * BATCH {
                return Ok(());
            }
        }
    }
}

pub fn held_keys(fd: RawFd) -> Vec<u8> {
    let mut keys = vec![0u8; KEY_BYTES];
    unsafe { libc::ioctl(fd, eviocgkey(keys.len() as u32), keys.as_mut_ptr()) };
    keys
}

pub fn is_held(keys: &[u8], code: u16) -> bool {
    bit(keys, code as usize)
}

/// A virtual input device created through /dev/uinput.
pub struct UinputDevice {
    file: File,
    pending: Vec<InputEvent>,
}

#[repr(C)]
struct UinputSetup {
    bustype: u16,
    vendor: u16,
    product: u16,
    version: u16,
    name: [u8; 80],
    ff_effects_max: u32,
}

pub const BUS_VIRTUAL: u16 = 0x06;

impl UinputDevice {
    pub fn create(name: &str, keys: &[u16], rel: &[u16]) -> io::Result<Self> {
        let file = OpenOptions::new().write(true).custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC).open("/dev/uinput")?;
        let fd = file.as_raw_fd();
        let check = |result: libc::c_int| if result < 0 { Err(io::Error::last_os_error()) } else { Ok(()) };
        unsafe {
            check(libc::ioctl(fd, UI_SET_EVBIT, ev::EV_SYN as libc::c_ulong))?;
            if !keys.is_empty() {
                check(libc::ioctl(fd, UI_SET_EVBIT, ev::EV_KEY as libc::c_ulong))?;
                for key in keys {
                    check(libc::ioctl(fd, UI_SET_KEYBIT, *key as libc::c_ulong))?;
                }
            }
            if !rel.is_empty() {
                check(libc::ioctl(fd, UI_SET_EVBIT, ev::EV_REL as libc::c_ulong))?;
                for axis in rel {
                    check(libc::ioctl(fd, UI_SET_RELBIT, *axis as libc::c_ulong))?;
                }
            }
            let mut setup = UinputSetup { bustype: BUS_VIRTUAL, vendor: 0x1532, product: 0x00b4, version: 1, name: [0; 80], ff_effects_max: 0 };
            let bytes = name.as_bytes();
            setup.name[..bytes.len().min(79)].copy_from_slice(&bytes[..bytes.len().min(79)]);
            check(libc::ioctl(fd, UI_DEV_SETUP, &setup as *const UinputSetup))?;
            check(libc::ioctl(fd, UI_DEV_CREATE))?;
        }
        Ok(UinputDevice { file, pending: Vec::with_capacity(32) })
    }

    pub fn push(&mut self, kind: u16, code: u16, value: i32) {
        self.pending.push(event(kind, code, value));
    }

    /// Ends the frame with SYN_REPORT and writes it in one call.
    pub fn flush(&mut self) {
        if self.pending.is_empty() {
            return;
        }
        self.pending.push(event(ev::EV_SYN, ev::SYN_REPORT, 0));
        let size = std::mem::size_of::<InputEvent>() * self.pending.len();
        let mut offset = 0;
        let bytes = self.pending.as_ptr() as *const u8;
        while offset < size {
            let written = unsafe { libc::write(self.file.as_raw_fd(), bytes.add(offset) as *const libc::c_void, size - offset) };
            if written < 0 {
                let error = io::Error::last_os_error();
                if error.raw_os_error() == Some(libc::EINTR) {
                    continue;
                }
                eprintln!("[Input] Virtual device write failed: {error}");
                break;
            }
            offset += written as usize;
        }
        self.pending.clear();
    }
}

impl Drop for UinputDevice {
    fn drop(&mut self) {
        unsafe { libc::ioctl(self.file.as_raw_fd(), UI_DEV_DESTROY) };
    }
}

/// Every keyboard key the virtual keyboard may send.
pub fn keyboard_keys() -> Vec<u16> {
    (1..ev::BTN_MISC).chain(ev::KEY_OK..=ev::KEY_MAX).collect()
}

pub fn pointer_buttons() -> Vec<u16> {
    (ev::BTN_LEFT..=ev::BTN_TASK).collect()
}

pub fn pointer_axes() -> Vec<u16> {
    vec![ev::REL_X, ev::REL_Y, ev::REL_HWHEEL, ev::REL_WHEEL, ev::REL_WHEEL_HI_RES, ev::REL_HWHEEL_HI_RES]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ioctl_numbers_match_the_kernel_headers() {
        assert_eq!(EVIOCGRAB, 0x40044590);
        assert_eq!(eviocgbit(1, 96), 0x80604521);
        assert_eq!(eviocgkey(96), 0x80604518);
        assert_eq!(UI_SET_EVBIT, 0x40045564);
        assert_eq!(UI_SET_KEYBIT, 0x40045565);
        assert_eq!(UI_SET_RELBIT, 0x40045566);
        assert_eq!(UI_DEV_SETUP, 0x405c5503);
        assert_eq!(UI_DEV_CREATE, 0x5501);
        assert_eq!(UI_DEV_DESTROY, 0x5502);
        assert_eq!(std::mem::size_of::<UinputSetup>(), 92);
        assert_eq!(std::mem::size_of::<InputEvent>(), 24);
        assert_eq!(KEY_BYTES, 96);
    }

    #[test]
    fn capability_classification() {
        let mut caps = Capabilities { keys: vec![0; KEY_BYTES], rel: vec![0; 2] };
        caps.keys[0x110 / 8] |= 1 << (0x110 % 8);
        caps.rel[0] = 1;
        assert!(caps.is_pointer() && !caps.is_keyboard());
        caps.keys[0] |= 1 << 2;
        assert!(caps.is_keyboard());
    }
}
