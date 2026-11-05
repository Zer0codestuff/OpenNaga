//! Device discovery through sysfs. Reading sysfs never opens a device.

use super::descriptor::{self, DescriptorSummary};
use std::fs;
use std::path::{Path, PathBuf};

pub const RAZER_VENDOR: u32 = 0x1532;
pub const RECEIVER_PRODUCT: u32 = 0x00b4;
/// Observed as "Naga V2 HS" over Bluetooth Low Energy.
pub const BLUETOOTH_VENDOR: u32 = 0x068e;
pub const BLUETOOTH_PRODUCT: u32 = 0x00b5;

pub const BUS_USB: u32 = 0x03;
pub const BUS_BLUETOOTH: u32 = 0x05;

pub fn is_supported(vendor: u32, product: u32, name: &str) -> bool {
    if vendor == BLUETOOTH_VENDOR && product == BLUETOOTH_PRODUCT {
        return true;
    }
    vendor == RAZER_VENDOR && (product == RECEIVER_PRODUCT || name.to_lowercase().contains("naga"))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HidDevice {
    /// Kernel name, for example `0003:1532:00B4.0011`.
    pub kernel_name: String,
    pub sysfs: PathBuf,
    pub bus: u32,
    pub vendor: u32,
    pub product: u32,
    pub name: String,
    /// Physical path, for example `usb-0000:00:14.0-10.4.3/input0`.
    pub phys: String,
    pub hidraw: Option<PathBuf>,
    /// evdev nodes created from this HID device.
    pub events: Vec<PathBuf>,
    pub descriptor: DescriptorSummary,
}

impl HidDevice {
    pub fn transport(&self) -> &'static str {
        match self.bus {
            BUS_USB => "USB receiver",
            BUS_BLUETOOTH => "Bluetooth",
            _ => "Other",
        }
    }

    /// The USB interface number from the physical path, when present.
    #[cfg(test)]
    pub fn interface(&self) -> Option<u32> {
        self.phys.rsplit_once("/input").and_then(|(_, n)| n.parse().ok())
    }

    /// Receiver location without the interface, stable across reconnects
    /// to the same port.
    pub fn location(&self) -> String {
        self.phys.rsplit_once("/input").map(|(p, _)| p.to_string()).unwrap_or_else(|| self.phys.clone())
    }

    pub fn is_naga(&self) -> bool {
        is_supported(self.vendor, self.product, &self.name)
    }
}

/// Parses `HID_ID=0003:00001532:000000B4`.
pub fn parse_hid_id(value: &str) -> Option<(u32, u32, u32)> {
    let mut parts = value.split(':');
    let bus = u32::from_str_radix(parts.next()?, 16).ok()?;
    let vendor = u32::from_str_radix(parts.next()?, 16).ok()?;
    let product = u32::from_str_radix(parts.next()?, 16).ok()?;
    Some((bus, vendor, product))
}

fn read_uevent(path: &Path) -> Vec<(String, String)> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| line.split_once('=').map(|(k, v)| (k.to_string(), v.to_string())))
        .collect()
}

pub fn hid_devices_under(root: &Path) -> Vec<HidDevice> {
    let Ok(entries) = fs::read_dir(root) else { return Vec::new() };
    let mut devices = Vec::new();
    for entry in entries.flatten() {
        let sysfs = entry.path();
        let uevent = read_uevent(&sysfs.join("uevent"));
        let get = |key: &str| uevent.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone()).unwrap_or_default();
        let Some((bus, vendor, product)) = parse_hid_id(&get("HID_ID")) else { continue };
        let name = get("HID_NAME");
        let hidraw = fs::read_dir(sysfs.join("hidraw"))
            .ok()
            .and_then(|mut d| d.next())
            .and_then(|e| e.ok())
            .map(|e| PathBuf::from("/dev").join(e.file_name()));
        let mut events = Vec::new();
        if let Ok(inputs) = fs::read_dir(sysfs.join("input")) {
            for input in inputs.flatten() {
                if let Ok(nodes) = fs::read_dir(input.path()) {
                    for node in nodes.flatten() {
                        let file = node.file_name().to_string_lossy().to_string();
                        if file.starts_with("event") {
                            events.push(PathBuf::from("/dev/input").join(file));
                        }
                    }
                }
            }
        }
        events.sort();
        let descriptor = fs::read(sysfs.join("report_descriptor")).map(|d| descriptor::parse(&d)).unwrap_or_default();
        devices.push(HidDevice {
            kernel_name: entry.file_name().to_string_lossy().to_string(),
            sysfs,
            bus,
            vendor,
            product,
            name,
            phys: get("HID_PHYS"),
            hidraw,
            events,
            descriptor,
        });
    }
    devices.sort_by(|a, b| a.kernel_name.cmp(&b.kernel_name));
    devices
}

/// Every Naga HID device currently known to the kernel.
pub fn naga_devices() -> Vec<HidDevice> {
    hid_devices_under(Path::new("/sys/bus/hid/devices")).into_iter().filter(|d| d.is_naga()).collect()
}

/// Name of an evdev node from sysfs, without opening it.
pub fn event_name(event: &Path) -> Option<String> {
    let file = event.file_name()?.to_str()?;
    fs::read_to_string(format!("/sys/class/input/{file}/device/name")).ok().map(|s| s.trim().to_string())
}

/// Bluetooth battery as exposed by the kernel's HID battery driver or BlueZ.
pub fn bluetooth_battery(devices: &[HidDevice]) -> Option<i32> {
    for device in devices.iter().filter(|d| d.bus == BUS_BLUETOOTH) {
        if let Ok(entries) = fs::read_dir(device.sysfs.join("power_supply")) {
            for entry in entries.flatten() {
                if let Ok(text) = fs::read_to_string(entry.path().join("capacity")) {
                    if let Ok(value) = text.trim().parse::<i32>() {
                        return Some(value.clamp(0, 100));
                    }
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identities() {
        assert!(is_supported(0x1532, 0x00b4, ""));
        assert!(is_supported(0x068e, 0x00b5, ""));
        assert!(is_supported(0x1532, 0x0099, "Razer Naga Pro"));
        assert!(!is_supported(0x1532, 0x0099, "Razer DeathAdder"));
        assert!(!is_supported(0x068e, 0x0001, "Naga"));
        assert_eq!(parse_hid_id("0003:00001532:000000B4"), Some((3, 0x1532, 0xb4)));
        assert_eq!(parse_hid_id("junk"), None);
    }

    #[test]
    fn reads_a_fake_sysfs_tree() {
        let root = crate::model::store::tests::temp_dir("sysfs");
        let device = root.join("0003:1532:00B4.0011");
        fs::create_dir_all(device.join("hidraw/hidraw16")).unwrap();
        fs::create_dir_all(device.join("input/input33/event256")).unwrap();
        fs::write(
            device.join("uevent"),
            "DRIVER=hid-generic\nHID_ID=0003:00001532:000000B4\nHID_NAME=Razer Razer Naga V2 HyperSpeed\nHID_PHYS=usb-0000:00:14.0-10.4.3/input0\n",
        )
        .unwrap();
        fs::write(device.join("report_descriptor"), crate::hw::descriptor::tests::hex(crate::hw::descriptor::tests::INTERFACE0)).unwrap();
        let found = hid_devices_under(&root);
        assert_eq!(found.len(), 1);
        let d = &found[0];
        assert_eq!(d.hidraw.as_deref(), Some(Path::new("/dev/hidraw16")));
        assert_eq!(d.events, vec![PathBuf::from("/dev/input/event256")]);
        assert_eq!(d.interface(), Some(0));
        assert_eq!(d.location(), "usb-0000:00:14.0-10.4.3");
        assert_eq!(d.transport(), "USB receiver");
        assert_eq!(d.descriptor.feature_bytes(0), Some(90));
    }
}
