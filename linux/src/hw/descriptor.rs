//! Minimal HID report descriptor reader. It finds the receiver interface
//! that carries the 90-byte feature report and classifies input reports,
//! using only the descriptor the kernel exposes in sysfs (no device I/O).

use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DescriptorSummary {
    /// (usage page, usage) of every top-level collection, in order.
    pub collections: Vec<(u32, u32)>,
    /// Report ID (0 when the device uses none) to size in bits.
    pub input_bits: BTreeMap<u8, u32>,
    pub feature_bits: BTreeMap<u8, u32>,
    pub uses_report_ids: bool,
}

impl DescriptorSummary {
    pub fn primary_usage(&self) -> Option<(u32, u32)> {
        self.collections.first().copied()
    }

    /// Feature report size in bytes, without the report ID byte.
    pub fn feature_bytes(&self, report_id: u8) -> Option<u32> {
        self.feature_bits.get(&report_id).map(|bits| bits.div_ceil(8))
    }
}

#[derive(Clone, Copy, Default)]
struct Globals {
    usage_page: u32,
    report_size: u32,
    report_count: u32,
    report_id: u8,
}

pub fn parse(bytes: &[u8]) -> DescriptorSummary {
    let mut summary = DescriptorSummary::default();
    let mut globals = Globals::default();
    let mut stack: Vec<Globals> = Vec::new();
    let mut local_usages: Vec<u32> = Vec::new();
    let mut depth = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        let prefix = bytes[i];
        if prefix == 0xfe {
            // Long item: size, tag, data.
            let size = bytes.get(i + 1).copied().unwrap_or(0) as usize;
            i += 3 + size;
            continue;
        }
        let size = match prefix & 3 {
            3 => 4,
            n => n as usize,
        };
        let kind = (prefix >> 2) & 3;
        let tag = prefix >> 4;
        if i + 1 + size > bytes.len() {
            break;
        }
        let data = &bytes[i + 1..i + 1 + size];
        let value = data.iter().rev().fold(0u32, |acc, b| (acc << 8) | *b as u32);
        i += 1 + size;
        match (kind, tag) {
            // Main items.
            (0, 0x8) | (0, 0xb) => {
                let bits = globals.report_size.saturating_mul(globals.report_count);
                let map = if tag == 0x8 { &mut summary.input_bits } else { &mut summary.feature_bits };
                *map.entry(globals.report_id).or_insert(0) += bits;
                local_usages.clear();
            }
            (0, 0x9) => local_usages.clear(),
            (0, 0xa) => {
                if depth == 0 {
                    let usage = local_usages.first().copied().unwrap_or(0);
                    // A 4-byte usage carries its own page in the high half.
                    let (page, usage) = if size == 4 || usage > 0xffff { (usage >> 16, usage & 0xffff) } else { (globals.usage_page, usage) };
                    summary.collections.push((page, usage));
                }
                depth += 1;
                local_usages.clear();
            }
            (0, 0xc) => depth = depth.saturating_sub(1),
            // Global items.
            (1, 0x0) => globals.usage_page = value,
            (1, 0x7) => globals.report_size = value,
            (1, 0x8) => {
                globals.report_id = value as u8;
                summary.uses_report_ids = true;
            }
            (1, 0x9) => globals.report_count = value,
            (1, 0xa) => stack.push(globals),
            (1, 0xb) => {
                if let Some(saved) = stack.pop() {
                    globals = saved;
                }
            }
            // Local items.
            (2, 0x0) => local_usages.push(if size == 4 { value } else { value & 0xffff }),
            _ => {}
        }
    }
    summary
}

#[cfg(test)]
pub mod tests {
    use super::*;

    pub fn hex(text: &str) -> Vec<u8> {
        text.split_whitespace().map(|b| u8::from_str_radix(b, 16).unwrap()).collect()
    }

    /// Interface 0 of receiver 1532:00b4, read from sysfs on 2026-11-02.
    pub const INTERFACE0: &str = "05 01 09 02 a1 01 09 01 a1 00 05 09 19 01 29 05 15 00 25 01 75 01 95 05 81 02 75 01 95 0b 81 03 06 00 ff 05 0c 0a 38 02 15 81 25 7f 75 08 95 01 81 06 05 01 09 38 15 81 25 7f 75 08 95 01 81 06 09 30 09 31 16 00 80 26 ff 7f 75 10 95 02 81 06 c0 06 00 ff 09 02 15 00 25 01 75 08 95 5a b1 01 c0";
    /// Interface 1 (keyboard, mouse, consumer, system and vendor reports).
    pub const INTERFACE1: &str = "05 01 09 06 a1 01 85 01 05 07 19 e0 29 e7 15 00 25 01 75 01 95 08 81 02 19 00 2a ff 00 15 00 26 ff 00 75 08 95 0e 81 00 05 08 19 01 29 03 15 00 25 01 75 01 95 03 91 02 95 05 91 01 c0 05 01 09 02 a1 01 85 0a 09 01 a1 00 05 09 19 01 29 06 15 00 25 01 75 01 95 06 81 02 75 01 95 02 81 03 06 00 ff 09 40 75 08 95 02 15 81 25 7f 81 02 05 01 09 38 15 81 25 7f 75 08 95 01 81 06 09 30 09 31 16 00 80 26 ff 7f 75 10 95 02 81 06 c0 c0 05 0c 09 01 a1 01 85 02 19 00 2a 3c 02 15 00 26 3c 02 95 01 75 10 81 00 75 08 95 0d 81 01 c0 05 01 09 80 a1 01 85 03 19 81 29 83 15 00 25 01 75 01 95 03 81 02 95 05 81 01 75 08 95 0e 81 01 c0 05 01 09 00 a1 01 85 04 09 03 15 00 26 ff 00 35 00 46 ff 00 75 08 95 0f 81 00 c0 05 01 09 00 a1 01 85 05 09 03 15 00 26 ff 00 35 00 46 ff 00 75 08 95 0f 81 00 c0 05 01 09 00 a1 01 85 08 09 03 15 00 26 ff 00 35 00 46 ff 00 75 08 95 0f 81 00 c0 05 01 09 00 a1 01 85 09 09 03 15 00 26 ff 00 35 00 46 ff 00 75 08 95 0f 81 00 c0";
    pub const INTERFACE2: &str = "05 01 09 06 a1 01 05 07 19 e0 29 e7 15 00 25 01 75 01 95 08 81 02 81 01 19 00 2a ff 00 15 00 26 ff 00 75 08 95 06 81 00 05 08 19 01 29 03 15 00 25 01 75 01 95 03 91 02 95 05 91 01 c0";

    #[test]
    fn control_interface_has_the_90_byte_feature_report() {
        let summary = parse(&hex(INTERFACE0));
        assert_eq!(summary.primary_usage(), Some((1, 2)));
        assert_eq!(summary.feature_bytes(0), Some(90));
        assert!(!summary.uses_report_ids);
        assert_eq!(summary.input_bits.get(&0), Some(&64), "8-byte mouse report");
    }

    #[test]
    fn keyboard_interface_carries_vendor_report_4() {
        let summary = parse(&hex(INTERFACE1));
        assert_eq!(summary.primary_usage(), Some((1, 6)));
        assert!(summary.uses_report_ids);
        assert_eq!(summary.feature_bytes(0), None);
        assert_eq!(summary.input_bits.get(&4), Some(&120), "15 data bytes after the ID");
        assert_eq!(summary.collections.len(), 8);
        assert_eq!(summary.collections[1], (1, 2));
        let third = parse(&hex(INTERFACE2));
        assert_eq!(third.primary_usage(), Some((1, 6)));
        assert!(third.feature_bits.is_empty());
    }
}
