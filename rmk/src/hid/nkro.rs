//! USB n-key rollover: the keyboard interface's report-protocol report is a
//! bitmap of every Keyboard/Keypad usage from 0x04 to 0xDD, so any number of
//! held keys reach the host. Boot protocol and Bluetooth keep the six-key
//! report, which [`NkroReport`] carries alongside the bitmap.
//!
//! The report descriptor follows the USB HID 1.11 keyboard layout and the
//! bitmap descriptor from Benson9954029/rmk `feat/nkro-usb-ble`
//! (048b2462bd, MIT OR Apache-2.0).

use core::sync::atomic::{AtomicU8, Ordering};

use embassy_usb::class::hid::HidProtocolMode;
use usbd_hid::descriptor::{AsInputReport, BufferOverflow, SerializedDescriptor};

use super::{KeyboardReport, Report};

/// The protocol the USB host selected for the keyboard interface. HID 1.11
/// 7.2.6: a device starts, and returns after a reset, in report protocol.
static KEYBOARD_PROTOCOL: AtomicU8 = AtomicU8::new(HidProtocolMode::Report as u8);

/// The keyboard interface's current protocol.
pub fn keyboard_protocol() -> HidProtocolMode {
    HidProtocolMode::from(KEYBOARD_PROTOCOL.load(Ordering::Relaxed))
}

/// Record the protocol the host selected (SET_PROTOCOL) or a reset restored.
pub fn set_keyboard_protocol(protocol: HidProtocolMode) {
    KEYBOARD_PROTOCOL.store(protocol as u8, Ordering::Relaxed);
}

/// The bytes the keyboard interface sends for `report` in `protocol`:
/// report protocol carries the bitmap, boot protocol the six-key report.
/// `None` for reports the keyboard interface does not carry.
pub fn keyboard_payload(
    report: &Report,
    protocol: HidProtocolMode,
    buf: &mut [u8; NKRO_REPORT_BYTES],
) -> Option<usize> {
    let written = match (report, protocol) {
        (Report::NkroKeyboardReport(r), HidProtocolMode::Report) => r.bitmap.serialize(buf),
        (Report::NkroKeyboardReport(r), HidProtocolMode::Boot) => r.six_key.serialize(buf),
        (Report::KeyboardReport(r), HidProtocolMode::Report) => NkroKeyboardReport::from_6kro(r).serialize(buf),
        (Report::KeyboardReport(r), HidProtocolMode::Boot) => r.serialize(buf),
        _ => return None,
    };
    written.ok()
}

/// Bytes in the bitmap: 218 usages (0x04..=0xDD), padded to whole bytes.
pub const NKRO_BITMAP_BYTES: usize = 28;
/// Bytes in the report-protocol report: modifiers, reserved, bitmap.
pub const NKRO_REPORT_BYTES: usize = 2 + NKRO_BITMAP_BYTES;

const FIRST_USAGE: u8 = 0x04;
const LAST_USAGE: u8 = 0xDD;

/// The report-protocol keyboard report with every held key.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct NkroKeyboardReport {
    pub modifier: u8,
    pub keycodes: [u8; NKRO_BITMAP_BYTES],
}

impl NkroKeyboardReport {
    /// A report with `modifier` and each of `usages` held. Usages outside
    /// 0x04..=0xDD, such as `No`, are left out.
    pub fn from_usages(modifier: u8, usages: impl IntoIterator<Item = u8>) -> Self {
        let mut report = Self {
            modifier,
            ..Self::default()
        };
        for usage in usages.into_iter().filter(|u| (FIRST_USAGE..=LAST_USAGE).contains(u)) {
            let bit = usize::from(usage - FIRST_USAGE);
            report.keycodes[bit / 8] |= 1 << (bit % 8);
        }
        report
    }

    /// The bitmap form of a six-key report, for the report protocol while
    /// rollover is off.
    pub fn from_6kro(report: &KeyboardReport) -> Self {
        Self::from_usages(report.modifier, report.keycodes)
    }
}

impl SerializedDescriptor for NkroKeyboardReport {
    fn desc() -> &'static [u8] {
        &[
            0x05, 0x01, // Usage Page (Generic Desktop)
            0x09, 0x06, // Usage (Keyboard)
            0xA1, 0x01, // Collection (Application)
            0x05, 0x07, //   Usage Page (Keyboard)
            0x19, 0xE0, //   Usage Minimum (Left Control)
            0x29, 0xE7, //   Usage Maximum (Right GUI)
            0x15, 0x00, //   Logical Minimum (0)
            0x25, 0x01, //   Logical Maximum (1)
            0x75, 0x01, //   Report Size (1)
            0x95, 0x08, //   Report Count (8)
            0x81, 0x02, //   Input (Data, Variable, Absolute): modifiers
            0x95, 0x01, //   Report Count (1)
            0x75, 0x08, //   Report Size (8)
            0x81, 0x01, //   Input (Constant): reserved byte
            0x05, 0x08, //   Usage Page (LEDs)
            0x19, 0x01, //   Usage Minimum (Num Lock)
            0x29, 0x05, //   Usage Maximum (Kana)
            0x95, 0x05, //   Report Count (5)
            0x75, 0x01, //   Report Size (1)
            0x91, 0x02, //   Output (Data, Variable, Absolute): LEDs
            0x95, 0x01, //   Report Count (1)
            0x75, 0x03, //   Report Size (3)
            0x91, 0x01, //   Output (Constant): LED padding
            0x05, 0x07, //   Usage Page (Keyboard)
            0x19, 0x04, //   Usage Minimum (A)
            0x29, 0xDD, //   Usage Maximum (0xDD)
            0x15, 0x00, //   Logical Minimum (0)
            0x25, 0x01, //   Logical Maximum (1)
            0x75, 0x01, //   Report Size (1)
            0x95, 0xDA, //   Report Count (218)
            0x81, 0x02, //   Input (Data, Variable, Absolute): bitmap
            0x95, 0x06, //   Report Count (6)
            0x81, 0x01, //   Input (Constant): bitmap padding
            0xC0, // End Collection
        ]
    }
}

impl AsInputReport for NkroKeyboardReport {
    fn serialize(&self, buffer: &mut [u8]) -> Result<usize, BufferOverflow> {
        if buffer.len() < NKRO_REPORT_BYTES {
            return Err(BufferOverflow);
        }
        buffer[0] = self.modifier;
        buffer[1] = 0;
        buffer[2..NKRO_REPORT_BYTES].copy_from_slice(&self.keycodes);
        Ok(NKRO_REPORT_BYTES)
    }
}

/// What the keyboard sends while rollover is on: every held key, and the
/// six-key report for boot protocol and the transports that carry six.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct NkroReport {
    pub bitmap: NkroKeyboardReport,
    pub six_key: KeyboardReport,
}

#[cfg(test)]
mod tests {
    use embassy_usb::class::hid::HidProtocolMode;
    use rmk_types::keycode::HidKeyCode;
    use usbd_hid::descriptor::AsInputReport;

    use super::{NKRO_REPORT_BYTES, NkroKeyboardReport, NkroReport, keyboard_payload};
    use crate::hid::{KeyboardReport, Report};

    fn six_key(keys: [u8; 6]) -> KeyboardReport {
        KeyboardReport {
            modifier: 0x02,
            reserved: 0,
            leds: 0,
            keycodes: keys,
        }
    }

    fn payload(report: &Report, protocol: HidProtocolMode) -> std::vec::Vec<u8> {
        let mut buf = [0u8; NKRO_REPORT_BYTES];
        let len = keyboard_payload(report, protocol, &mut buf).expect("a keyboard report");
        buf[..len].to_vec()
    }

    fn serialized(report: impl AsInputReport) -> std::vec::Vec<u8> {
        let mut buf = [0u8; 32];
        let len = report.serialize(&mut buf).unwrap();
        buf[..len].to_vec()
    }

    // Bluetooth and the dongle carry the six-key report (the user's choice,
    // 2026-11-08: F13-F24 keep working there); they send a report through its
    // generic serialization.
    // "Serialized as a plain report, an NKRO report is its six-key form."
    #[test]
    fn an_nkro_report_serializes_as_its_six_key_form() {
        let six = six_key([0x04, 0x05, 0x06, 0x07, 0x08, 0x09]);
        let bitmap = NkroKeyboardReport::from_usages(0x02, (0x04..=0x0A).collect::<std::vec::Vec<u8>>());
        let report = Report::NkroKeyboardReport(NkroReport { bitmap, six_key: six });
        assert_eq!(serialized(report), serialized(six));
    }

    // Spec: USB HID 1.11 7.2.6 and 4.2: in report protocol the host reads the
    // report descriptor's layout, here the bitmap.
    // "Report protocol sends an NKRO report's bitmap."
    #[test]
    fn report_protocol_sends_the_bitmap() {
        let bitmap = NkroKeyboardReport::from_usages(0x02, (0x04..=0x0A).collect::<std::vec::Vec<u8>>());
        let report = Report::NkroKeyboardReport(NkroReport {
            bitmap,
            six_key: six_key([0x04; 6]),
        });
        assert_eq!(payload(&report, HidProtocolMode::Report), serialized(bitmap));
    }

    // Same layout while rollover is off: the six-key report, as a bitmap.
    // "Report protocol sends a six-key report in the bitmap layout."
    #[test]
    fn report_protocol_sends_a_six_key_report_as_a_bitmap() {
        let six = six_key([0x04, 0x05, 0, 0, 0, 0]);
        let report = Report::KeyboardReport(six);
        assert_eq!(
            payload(&report, HidProtocolMode::Report),
            serialized(NkroKeyboardReport::from_6kro(&six))
        );
    }

    // Spec: USB HID 1.11 appendix B.1: boot protocol is the 8-byte keyboard
    // report the BIOS expects.
    // "Boot protocol sends the six-key report."
    #[test]
    fn boot_protocol_sends_the_six_key_report() {
        let six = six_key([0x04, 0x05, 0x06, 0x07, 0x08, 0x09]);
        let bitmap = NkroKeyboardReport::from_usages(0x02, (0x04..=0x0A).collect::<std::vec::Vec<u8>>());
        let report = Report::NkroKeyboardReport(NkroReport { bitmap, six_key: six });
        assert_eq!(payload(&report, HidProtocolMode::Boot), serialized(six));
    }

    // Spec: USB HID 1.11 and HID Usage Tables, Keyboard/Keypad page; the
    // descriptor above maps usage 0x04 to bitmap bit 0, LSB first per byte.
    // "A held key sets the bit for its usage."
    #[test]
    fn a_held_key_sets_its_usage_bit() {
        let report = NkroKeyboardReport::from_usages(0, [HidKeyCode::A as u8]);
        assert_eq!(report.keycodes[0], 0b0000_0001);
    }

    // Usage 0x68 (F13) is bit 100: byte 12, bit 4. The bitmap reaches 0xDD,
    // so keys past the 0x67 Bluetooth NKRO limit still count.
    // "F13 is in the bitmap."
    #[test]
    fn f13_is_in_the_bitmap() {
        let report = NkroKeyboardReport::from_usages(0, [HidKeyCode::F13 as u8]);
        assert_eq!(report.keycodes[12], 0b0001_0000);
    }

    // The descriptor's input fields: 8 modifier bits, a reserved byte, then
    // the 218-bit bitmap padded to 28 bytes.
    // "The report is the modifiers, a reserved byte, then the bitmap."
    #[test]
    fn the_report_serializes_modifiers_reserved_then_bitmap() {
        let report = NkroKeyboardReport::from_usages(0x02, [HidKeyCode::A as u8]);
        let mut bytes = [0u8; 32];
        let len = report.serialize(&mut bytes).unwrap();
        let mut expected = [0u8; NKRO_REPORT_BYTES];
        expected[0] = 0x02;
        expected[2] = 0b0000_0001;
        assert_eq!(&bytes[..len], &expected);
    }

    // With rollover off, report protocol still uses this layout, so a six-key
    // report converts into it.
    // "A six-key report's keys and modifiers carry over into the bitmap."
    #[test]
    fn a_six_key_report_converts_into_the_bitmap() {
        let six = KeyboardReport {
            modifier: 0x02,
            reserved: 0,
            leds: 0,
            keycodes: [HidKeyCode::A as u8, HidKeyCode::F13 as u8, 0, 0, 0, 0],
        };
        let expected = NkroKeyboardReport::from_usages(0x02, [HidKeyCode::A as u8, HidKeyCode::F13 as u8]);
        assert_eq!(NkroKeyboardReport::from_6kro(&six), expected);
    }
}
