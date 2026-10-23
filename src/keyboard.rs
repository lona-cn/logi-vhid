//! Keyboard reports, HID key codes, keyboard layouts, and Unicode text input.

use std::thread::sleep;
use std::time::Duration;

use crate::device::ReportSink;
use crate::error::{Error, Result};
use crate::ffi::{
    GetKeyState, InputKeyboard, KeyInput, LoadKeyboardLayoutW, MapVirtualKeyExW, SendInput,
    ToUnicodeEx, VkKeyScanExW, INPUT_KEYBOARD, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE,
};
use crate::protocol::{KeyboardReport, IOCTL_KEYBOARD_REPORT};

/// Keyboard HID usage ids (HID Usage Tables, Keyboard/Keypad page 0x07).
pub mod hid {
    pub const A: u8 = 0x04;
    pub const B: u8 = 0x05;
    pub const C: u8 = 0x06;
    pub const D: u8 = 0x07;
    pub const E: u8 = 0x08;
    pub const F: u8 = 0x09;
    pub const G: u8 = 0x0A;
    pub const H: u8 = 0x0B;
    pub const I: u8 = 0x0C;
    pub const J: u8 = 0x0D;
    pub const K: u8 = 0x0E;
    pub const L: u8 = 0x0F;
    pub const M: u8 = 0x10;
    pub const N: u8 = 0x11;
    pub const O: u8 = 0x12;
    pub const P: u8 = 0x13;
    pub const Q: u8 = 0x14;
    pub const R: u8 = 0x15;
    pub const S: u8 = 0x16;
    pub const T: u8 = 0x17;
    pub const U: u8 = 0x18;
    pub const V: u8 = 0x19;
    pub const W: u8 = 0x1A;
    pub const X: u8 = 0x1B;
    pub const Y: u8 = 0x1C;
    pub const Z: u8 = 0x1D;

    pub const NUM_1: u8 = 0x1E;
    pub const NUM_2: u8 = 0x1F;
    pub const NUM_3: u8 = 0x20;
    pub const NUM_4: u8 = 0x21;
    pub const NUM_5: u8 = 0x22;
    pub const NUM_6: u8 = 0x23;
    pub const NUM_7: u8 = 0x24;
    pub const NUM_8: u8 = 0x25;
    pub const NUM_9: u8 = 0x26;
    pub const NUM_0: u8 = 0x27;

    pub const ENTER: u8 = 0x28;
    pub const ESCAPE: u8 = 0x29;
    pub const BACKSPACE: u8 = 0x2A;
    pub const TAB: u8 = 0x2B;
    pub const SPACE: u8 = 0x2C;
    pub const MINUS: u8 = 0x2D;
    pub const EQUAL: u8 = 0x2E;
    pub const LEFT_BRACKET: u8 = 0x2F;
    pub const RIGHT_BRACKET: u8 = 0x30;
    pub const BACKSLASH: u8 = 0x31;
    pub const SEMICOLON: u8 = 0x33;
    pub const APOSTROPHE: u8 = 0x34;
    pub const GRAVE: u8 = 0x35;
    pub const COMMA: u8 = 0x36;
    pub const PERIOD: u8 = 0x37;
    pub const SLASH: u8 = 0x38;
    pub const CAPS_LOCK: u8 = 0x39;

    pub const F1: u8 = 0x3A;
    pub const F2: u8 = 0x3B;
    pub const F3: u8 = 0x3C;
    pub const F4: u8 = 0x3D;
    pub const F5: u8 = 0x3E;
    pub const F6: u8 = 0x3F;
    pub const F7: u8 = 0x40;
    pub const F8: u8 = 0x41;
    pub const F9: u8 = 0x42;
    pub const F10: u8 = 0x43;
    pub const F11: u8 = 0x44;
    pub const F12: u8 = 0x45;
    pub const F24: u8 = 0x73;

    pub const INSERT: u8 = 0x49;
    pub const HOME: u8 = 0x4A;
    pub const PAGE_UP: u8 = 0x4B;
    pub const DELETE: u8 = 0x4C;
    pub const END: u8 = 0x4D;
    pub const PAGE_DOWN: u8 = 0x4E;
    pub const RIGHT: u8 = 0x4F;
    pub const LEFT: u8 = 0x50;
    pub const DOWN: u8 = 0x51;
    pub const UP: u8 = 0x52;
    pub const NUM_LOCK: u8 = 0x53;

    pub const LEFT_CTRL: u8 = 0xE0;
    pub const LEFT_SHIFT: u8 = 0xE1;
    pub const LEFT_ALT: u8 = 0xE2;
    pub const LEFT_GUI: u8 = 0xE3;
    pub const RIGHT_CTRL: u8 = 0xE4;
    pub const RIGHT_SHIFT: u8 = 0xE5;
    pub const RIGHT_ALT: u8 = 0xE6;
    pub const RIGHT_GUI: u8 = 0xE7;
}

#[derive(Clone, Copy)]
pub(crate) struct KeyboardState {
    modifiers: u8,
    pressed: [u8; 6],
}

/// Keyboard layout used by HID text input.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeyboardLayout {
    Us,
    Uk,
    De,
    Fr,
}

impl KeyboardLayout {
    fn klid(self) -> &'static str {
        match self {
            Self::Us => "00000409",
            Self::Uk => "00000809",
            Self::De => "00000407",
            Self::Fr => "0000040C",
        }
    }
}

/// How [`crate::LogitechVirtualHid::type_text`] should send text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextMode {
    /// Send virtual keyboard reports for the selected Windows keyboard layout.
    /// Returns an error for characters outside Unicode's Basic Multilingual
    /// Plane or those requiring an accent key followed by a letter.
    Hid(KeyboardLayout),
    /// Win32 `SendInput` with `KEYEVENTF_UNICODE`.  Supports arbitrary Unicode
    /// but does not use the Logitech driver.
    Win32Unicode,
}

impl KeyboardState {
    pub(crate) fn new() -> Self {
        Self {
            modifiers: 0,
            pressed: [0; 6],
        }
    }

    fn contains(&self, usage: u8) -> bool {
        if (0xE0..=0xE7).contains(&usage) {
            self.modifiers & (1 << (usage - 0xE0)) != 0
        } else {
            usage != 0 && self.pressed.contains(&usage)
        }
    }

    pub(crate) fn key_down(&mut self, device: &impl ReportSink, usage: u8) -> Result<()> {
        if usage == 0 {
            return Err(Error::Unsupported(
                "HID usage 0 is reserved and cannot be pressed".to_string(),
            ));
        }
        let mut next = *self;
        if (0xE0..=0xE7).contains(&usage) {
            next.modifiers |= 1 << (usage - 0xE0);
        } else if !next.pressed.contains(&usage) {
            let slot = next
                .pressed
                .iter_mut()
                .find(|key| **key == 0)
                .ok_or_else(|| {
                    Error::Unsupported("all six keyboard report slots are full".to_string())
                })?;
            *slot = usage;
        }
        next.send(device)?;
        *self = next;
        Ok(())
    }

    pub(crate) fn key_up(&mut self, device: &impl ReportSink, usage: u8) -> Result<()> {
        if usage == 0 {
            return Ok(());
        }
        let mut next = *self;
        if (0xE0..=0xE7).contains(&usage) {
            next.modifiers &= !(1 << (usage - 0xE0));
        } else {
            next.pressed = [0; 6];
            let mut index = 0;
            for &key in &self.pressed {
                if key != 0 && key != usage {
                    next.pressed[index] = key;
                    index += 1;
                }
            }
        }
        next.send(device)?;
        *self = next;
        Ok(())
    }

    pub(crate) fn press(&mut self, device: &impl ReportSink, usage: u8) -> Result<()> {
        self.hold_key(device, usage, Duration::from_millis(20))
    }

    pub(crate) fn release_all(&mut self, device: &impl ReportSink) -> Result<()> {
        let next = Self::new();
        next.send(device)?;
        *self = next;
        Ok(())
    }

    /// Keep only saved keys and modifiers that are still held.
    pub(crate) fn retain_held(&mut self, other: &Self) {
        self.modifiers &= other.modifiers;
        let mut retained = [0; 6];
        let mut index = 0;
        for &key in &self.pressed {
            if key != 0 && other.pressed.contains(&key) {
                retained[index] = key;
                index += 1;
            }
        }
        self.pressed = retained;
    }

    /// Release keys pressed since `initial`, without pressing any released keys
    /// again. If sending the release fails, leave the state unchanged for retry.
    pub(crate) fn cleanup_since(&mut self, device: &impl ReportSink, initial: &Self) -> Result<()> {
        let mut next = *self;
        next.retain_held(initial);
        if next.modifiers != self.modifiers || next.pressed != self.pressed {
            next.send(device)?;
            *self = next;
        }
        Ok(())
    }

    fn finish_temporary(
        &mut self,
        device: &impl ReportSink,
        initial: &Self,
        result: Result<()>,
    ) -> Result<()> {
        let cleanup = self.cleanup_since(device, initial);
        result.and(cleanup)
    }

    pub(crate) fn press_combo(&mut self, device: &impl ReportSink, usages: &[u8]) -> Result<()> {
        if usages.len() > 14 {
            return Err(Error::Unsupported(
                "keyboard combo has more than 14 usages".to_string(),
            ));
        }
        let initial = *self;
        let mut result = Ok(());
        for &usage in usages {
            if let Err(err) = self.key_down(device, usage) {
                result = Err(err);
                break;
            }
        }
        if !usages.is_empty() {
            sleep(Duration::from_millis(20));
        }
        for &usage in usages.iter().rev() {
            if !initial.contains(usage) && self.contains(usage) {
                let released = self.key_up(device, usage);
                if result.is_ok() {
                    result = released;
                }
            }
        }
        self.finish_temporary(device, &initial, result)
    }

    pub(crate) fn hold_key(
        &mut self,
        device: &impl ReportSink,
        usage: u8,
        duration: Duration,
    ) -> Result<()> {
        let initial = *self;
        let result = (|| {
            self.key_down(device, usage)?;
            sleep(duration);
            if !initial.contains(usage) {
                self.key_up(device, usage)?;
            }
            Ok(())
        })();
        self.finish_temporary(device, &initial, result)
    }

    pub(crate) fn repeat_key(
        &mut self,
        device: &impl ReportSink,
        usage: u8,
        count: u32,
        interval: Duration,
    ) -> Result<()> {
        for index in 0..count {
            self.hold_key(device, usage, Duration::from_millis(8))?;
            if index + 1 < count {
                sleep(interval);
            }
        }
        Ok(())
    }

    /// Type text through HID reports using a Windows keyboard layout.
    pub(crate) fn type_text_layout(
        &mut self,
        device: &impl ReportSink,
        text: &str,
        layout: KeyboardLayout,
    ) -> Result<()> {
        if text.is_empty() {
            return Ok(());
        }
        let caps = caps_lock_state()?;
        self.type_layout(device, text, layout, caps, false)
    }

    fn type_layout(
        &mut self,
        device: &impl ReportSink,
        text: &str,
        layout: KeyboardLayout,
        caps: bool,
        ascii_only: bool,
    ) -> Result<()> {
        let hkl = load_layout(layout)?;
        let mut previous_cr = false;
        for (index, ch) in text.chars().enumerate() {
            if ch == '\n' && previous_cr {
                previous_cr = false;
                continue;
            }
            previous_cr = ch == '\r';
            if ascii_only && !matches!(ch, ' '..='~' | '\r' | '\n' | '\t') {
                return Err(Error::Unsupported(format!(
                    "unsupported character {ch:?} at index {index}; type_ascii accepts printable ASCII"
                )));
            }
            let (usage, shift, ctrl, alt) = match ch {
                '\n' | '\r' => (hid::ENTER, false, false, false),
                '\t' => (hid::TAB, false, false, false),
                _ => char_to_usage(ch, hkl, caps).ok_or_else(|| {
                    Error::Unsupported(format!(
                        "character {ch:?} at index {index} is not available without dead keys in {layout:?}"
                    ))
                })?,
            };
            self.type_one(device, usage, shift, ctrl, alt)?;
            if ascii_only {
                sleep(Duration::from_millis(8));
            }
        }
        Ok(())
    }

    fn type_one(
        &mut self,
        device: &impl ReportSink,
        usage: u8,
        shift: bool,
        ctrl: bool,
        alt: bool,
    ) -> Result<()> {
        let initial = *self;
        let mut modifiers = [0u8; 3];
        let mut count = 0;
        if shift {
            modifiers[count] = hid::LEFT_SHIFT;
            count += 1;
        }
        if ctrl {
            modifiers[count] = hid::LEFT_CTRL;
            count += 1;
        }
        if alt {
            modifiers[count] = if ctrl { hid::RIGHT_ALT } else { hid::LEFT_ALT };
            count += 1;
        }
        let mut result = (|| {
            for &modifier in &modifiers[..count] {
                self.key_down(device, modifier)?;
            }
            self.key_down(device, usage)?;
            sleep(Duration::from_millis(8));
            if !initial.contains(usage) {
                self.key_up(device, usage)?;
            }
            Ok(())
        })();
        for &modifier in modifiers[..count].iter().rev() {
            if !initial.contains(modifier) && self.contains(modifier) {
                let released = self.key_up(device, modifier);
                if result.is_ok() {
                    result = released;
                }
            }
        }
        self.finish_temporary(device, &initial, result)
    }

    /// Type printable ASCII using the US physical keyboard layout.
    pub(crate) fn type_ascii(&mut self, device: &impl ReportSink, text: &str) -> Result<()> {
        if text.is_empty() {
            return Ok(());
        }
        let caps = caps_lock_state()?;
        self.type_layout(device, text, KeyboardLayout::Us, caps, true)
    }

    fn send(&self, device: &impl ReportSink) -> Result<()> {
        let report = KeyboardReport {
            modifiers: self.modifiers,
            keys: self.pressed,
        };
        device.send_report(IOCTL_KEYBOARD_REPORT, &report.to_bytes())
    }
}

fn caps_lock_state() -> Result<bool> {
    // GetKeyState on a host GUI thread can describe an old input message.
    // A fresh reader gets an independent snapshot without pumping messages or
    // attaching to (and resetting) the host or foreground input queue.
    std::thread::Builder::new()
        .spawn(|| unsafe { GetKeyState(0x14) } & 1 != 0)
        .map_err(|error| Error::io("create Caps Lock state reader", error))?
        .join()
        .map_err(|_| Error::Protocol("Caps Lock state reader panicked".to_string()))
}

fn load_layout(layout: KeyboardLayout) -> Result<isize> {
    let mut name = [0u16; 9];
    for (slot, byte) in name.iter_mut().zip(layout.klid().bytes()) {
        *slot = u16::from(byte);
    }
    let hkl = unsafe { LoadKeyboardLayoutW(name.as_ptr(), 0) };
    if hkl == 0 {
        Err(Error::last_io("LoadKeyboardLayoutW"))
    } else {
        Ok(hkl)
    }
}

fn char_to_usage(ch: char, hkl: isize, caps: bool) -> Option<(u8, bool, bool, bool)> {
    // WCHAR is not a Unicode scalar: reject rather than truncate non-BMP text.
    let character = u16::try_from(ch as u32).ok()?;
    let scan = unsafe { VkKeyScanExW(character, hkl) };
    if scan == -1 {
        return None;
    }
    let value = scan as u16;
    let vk = (value & 0xFF) as u32;
    let modifiers = (value >> 8) as u8;
    if modifiers & !7 != 0 {
        return None;
    }
    let scan_code = unsafe { MapVirtualKeyExW(vk, 4, hkl) } & 0xFF;
    let usage = scan_to_usage(scan_code as u16)?;
    let ctrl = modifiers & 2 != 0;
    let alt = modifiers & 4 != 0;
    let preferred_shift = modifiers & 1 != 0;
    for shift in [preferred_shift, !preferred_shift] {
        let mut state = [0u8; 256];
        state[0x14] = u8::from(caps);
        if shift {
            state[0x10] = 0x80;
            state[0xA0] = 0x80;
        }
        if ctrl {
            state[0x11] = 0x80;
            state[0xA2] = 0x80;
        }
        if alt {
            state[0x12] = 0x80;
            state[if ctrl { 0xA5 } else { 0xA4 }] = 0x80;
        }
        let mut output = [0u16; 4];
        // Bit 2 prevents ToUnicodeEx from mutating the user's dead-key state.
        let count = unsafe {
            ToUnicodeEx(
                vk,
                scan_code,
                state.as_ptr(),
                output.as_mut_ptr(),
                4,
                4,
                hkl,
            )
        };
        if count == 1 && output[0] == character {
            return Some((usage, shift, ctrl, alt));
        }
    }
    None
}

fn scan_to_usage(scan: u16) -> Option<u8> {
    let usage = match scan {
        0x01 => 0x29,
        0x02..=0x0B => 0x1E + (scan as u8 - 0x02),
        0x0C => 0x2D,
        0x0D => 0x2E,
        0x0E => 0x2A,
        0x0F => 0x2B,
        0x10 => 0x14,
        0x11 => 0x1A,
        0x12 => 0x08,
        0x13 => 0x15,
        0x14 => 0x17,
        0x15 => 0x1C,
        0x16 => 0x18,
        0x17 => 0x0C,
        0x18 => 0x12,
        0x19 => 0x13,
        0x1A => 0x2F,
        0x1B => 0x30,
        0x1C => 0x28,
        0x1D => 0xE0,
        0x1E => 0x04,
        0x1F => 0x16,
        0x20 => 0x07,
        0x21 => 0x09,
        0x22 => 0x0A,
        0x23 => 0x0B,
        0x24 => 0x0D,
        0x25 => 0x0E,
        0x26 => 0x0F,
        0x27 => 0x33,
        0x28 => 0x34,
        0x29 => 0x35,
        0x2A => 0xE1,
        0x2B => 0x31,
        0x2C => 0x1D,
        0x2D => 0x1B,
        0x2E => 0x06,
        0x2F => 0x19,
        0x30 => 0x05,
        0x31 => 0x11,
        0x32 => 0x10,
        0x33 => 0x36,
        0x34 => 0x37,
        0x35 => 0x38,
        0x36 => 0xE5,
        0x37 => 0x55,
        0x38 => 0xE2,
        0x39 => 0x2C,
        0x3A => 0x39,
        0x3B..=0x44 => 0x3A + (scan as u8 - 0x3B),
        0x45 => 0x53,
        0x46 => 0x47,
        0x47..=0x4F => {
            [0x5F, 0x60, 0x61, 0x56, 0x5C, 0x5D, 0x5E, 0x57, 0x59][(scan - 0x47) as usize]
        }
        0x50..=0x53 => [0x5A, 0x5B, 0x62, 0x63][(scan - 0x50) as usize],
        0x56 => 0x64,
        0x57 => 0x44,
        0x58 => 0x45,
        _ => return None,
    };
    Some(usage)
}

/// Type arbitrary text with Win32 `SendInput` / `KEYEVENTF_UNICODE`.
///
/// If only a key-down is inserted, attempt its release before returning the
/// original send error, even if that release also fails.
pub(crate) fn type_unicode(text: &str) -> Result<()> {
    type_unicode_with(text, |inputs| {
        let sent = unsafe {
            SendInput(
                inputs.len() as u32,
                inputs.as_ptr() as *const u8,
                std::mem::size_of::<InputKeyboard>() as i32,
            )
        };
        if sent == inputs.len() as u32 {
            Ok(())
        } else {
            Err((sent, Error::last_io("SendInput")))
        }
    })
}

fn type_unicode_with(
    text: &str,
    mut send: impl FnMut(&[InputKeyboard]) -> std::result::Result<(), (u32, Error)>,
) -> Result<()> {
    for unit in text.encode_utf16() {
        let down = InputKeyboard {
            type_: INPUT_KEYBOARD,
            _pad: 0,
            ki: KeyInput {
                w_vk: 0,
                w_scan: unit,
                flags: KEYEVENTF_UNICODE,
                time: 0,
                extra_info: 0,
            },
            _tail: [0; 8],
        };
        let up = InputKeyboard {
            type_: INPUT_KEYBOARD,
            _pad: 0,
            ki: KeyInput {
                w_vk: 0,
                w_scan: unit,
                flags: KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
                time: 0,
                extra_info: 0,
            },
            _tail: [0; 8],
        };
        let inputs = [down, up];
        if let Err((sent, error)) = send(&inputs) {
            if sent == 1 {
                let _ = send(&inputs[1..]);
            }
            return Err(error);
        }
    }
    Ok(())
}

/// Translate a Win32 virtual-key code to a HID keyboard usage id.
///
/// This covers common keys; for full control use the raw usage ids in [`hid`].
pub fn vk_to_usage(vk: u16) -> Option<u8> {
    match vk {
        0x41..=0x5A => Some(0x04 + (vk as u8 - 0x41)),
        0x31..=0x39 => Some(0x1E + (vk as u8 - 0x31)),
        0x30 => Some(0x27),
        0x0D => Some(hid::ENTER),
        0x1B => Some(hid::ESCAPE),
        0x08 => Some(hid::BACKSPACE),
        0x09 => Some(hid::TAB),
        0x20 => Some(hid::SPACE),
        0x10 | 0xA0 => Some(hid::LEFT_SHIFT),
        0xA1 => Some(hid::RIGHT_SHIFT),
        0x11 | 0xA2 => Some(hid::LEFT_CTRL),
        0xA3 => Some(hid::RIGHT_CTRL),
        0x12 | 0xA4 => Some(hid::LEFT_ALT),
        0xA5 => Some(hid::RIGHT_ALT),
        0x5B => Some(hid::LEFT_GUI),
        0x5C => Some(hid::RIGHT_GUI),
        0x25 => Some(hid::LEFT),
        0x27 => Some(hid::RIGHT),
        0x26 => Some(hid::UP),
        0x28 => Some(hid::DOWN),
        0x2D => Some(hid::INSERT),
        0x24 => Some(hid::HOME),
        0x21 => Some(hid::PAGE_UP),
        0x2E => Some(hid::DELETE),
        0x23 => Some(hid::END),
        0x22 => Some(hid::PAGE_DOWN),
        0x70..=0x7B => Some(hid::F1 + (vk as u8 - 0x70)),
        0x87 => Some(hid::F24),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::Reports;

    struct UnicodeInput<'a> {
        outcomes: &'a [(u32, i32)],
        calls: usize,
        attempted: Vec<(u16, bool)>,
        accepted: Vec<(u16, bool)>,
        held: Vec<u16>,
    }

    impl<'a> UnicodeInput<'a> {
        fn new(outcomes: &'a [(u32, i32)]) -> Self {
            Self {
                outcomes,
                calls: 0,
                attempted: Vec::new(),
                accepted: Vec::new(),
                held: Vec::new(),
            }
        }

        fn send(&mut self, inputs: &[InputKeyboard]) -> std::result::Result<(), (u32, Error)> {
            let outcome = self.outcomes.get(self.calls).copied();
            self.calls += 1;
            let sent = outcome.map_or(inputs.len() as u32, |(sent, _)| sent);
            assert!(sent as usize <= inputs.len());
            for (index, input) in inputs.iter().enumerate() {
                assert_eq!(input.type_, INPUT_KEYBOARD);
                assert_eq!(input.ki.w_vk, 0);
                assert_eq!(input.ki.flags & !KEYEVENTF_KEYUP, KEYEVENTF_UNICODE);
                let event = (input.ki.w_scan, input.ki.flags & KEYEVENTF_KEYUP != 0);
                self.attempted.push(event);
                if index < sent as usize {
                    self.accepted.push(event);
                    if event.1 {
                        if let Some(index) = self.held.iter().position(|unit| *unit == event.0) {
                            self.held.remove(index);
                        }
                    } else {
                        self.held.push(event.0);
                    }
                }
            }
            if sent == inputs.len() as u32 {
                Ok(())
            } else {
                let code = outcome.expect("a partial insertion needs an OS error").1;
                Err((
                    sent,
                    Error::io("SendInput", std::io::Error::from_raw_os_error(code)),
                ))
            }
        }
    }

    #[test]
    fn unicode_partial_insertion_releases_key_and_stops_text() {
        let mut input = UnicodeInput::new(&[(1, 5)]);
        let error = type_unicode_with("ab", |events| input.send(events)).unwrap_err();
        assert!(matches!(
            error,
            Error::Io { source, .. } if source.raw_os_error() == Some(5)
        ));
        assert!(
            input.held.is_empty(),
            "a partially inserted key remains held"
        );
        assert_eq!(input.accepted, [(0x0061, false), (0x0061, true)]);
        assert_eq!(input.calls, 2);
    }

    #[test]
    fn unicode_failed_release_preserves_original_error_and_stops_text() {
        let mut input = UnicodeInput::new(&[(1, 5), (0, 123)]);
        let error = type_unicode_with("ab", |events| input.send(events)).unwrap_err();
        assert!(matches!(
            error,
            Error::Io { source, .. } if source.raw_os_error() == Some(5)
        ));
        assert_eq!(input.accepted, [(0x0061, false)]);
        assert_eq!(input.held, [0x0061]);
        assert_eq!(
            input.attempted,
            [(0x0061, false), (0x0061, true), (0x0061, true)]
        );
        assert_eq!(input.calls, 2);
    }

    #[test]
    fn unicode_zero_insertion_does_not_send_an_unmatched_release() {
        let mut input = UnicodeInput::new(&[(0, 5)]);
        let error = type_unicode_with("ab", |events| input.send(events)).unwrap_err();
        assert!(matches!(
            error,
            Error::Io { source, .. } if source.raw_os_error() == Some(5)
        ));
        assert!(input.accepted.is_empty());
        assert!(input.held.is_empty());
        assert_eq!(input.attempted, [(0x0061, false), (0x0061, true)]);
        assert_eq!(input.calls, 1);
    }

    #[test]
    fn unicode_success_pairs_surrogate_units_in_order() {
        let mut input = UnicodeInput::new(&[]);
        type_unicode_with("😀a", |events| input.send(events)).unwrap();
        assert_eq!(
            input.accepted,
            [
                (0xD83D, false),
                (0xD83D, true),
                (0xDE00, false),
                (0xDE00, true),
                (0x0061, false),
                (0x0061, true),
            ]
        );
        assert!(input.held.is_empty());
        assert_eq!(input.calls, 3);
    }

    #[test]
    fn virtual_keys_map_to_the_expected_hid_keys() {
        assert_eq!(vk_to_usage(0x41), Some(0x04));
        assert_eq!(vk_to_usage(0x5A), Some(0x1D));
        assert_eq!(vk_to_usage(0x31), Some(0x1E));
        assert_eq!(vk_to_usage(0x30), Some(0x27));
        assert_eq!(vk_to_usage(0x25), Some(0x50));
        assert_eq!(vk_to_usage(0x70), Some(0x3A));
        assert_eq!(vk_to_usage(0x87), Some(0x73));
        assert_eq!(vk_to_usage(0xFF), None);
    }

    #[test]
    fn scan_codes_map_to_physical_hid_keys() {
        assert_eq!(scan_to_usage(0x02), Some(0x1E));
        assert_eq!(scan_to_usage(0x1E), Some(0x04));
        assert_eq!(scan_to_usage(0x27), Some(0x33));
        assert_eq!(scan_to_usage(0x39), Some(0x2C));
        assert_eq!(scan_to_usage(0x56), Some(0x64));
        assert_eq!(scan_to_usage(0xFF), None);
    }

    #[test]
    fn win32_input_layout_matches_the_x64_abi() {
        use crate::ffi::{Input, InputKeyboard};
        assert_eq!(std::mem::size_of::<InputKeyboard>(), 40);
        assert_eq!(std::mem::size_of::<Input>(), 40);
    }

    #[test]
    fn rejected_press_is_not_in_the_next_report() {
        let reports = Reports::default();
        let mut keyboard = KeyboardState::new();
        reports.fail_on(&[1]);
        assert!(keyboard.key_down(&reports, hid::A).is_err());
        keyboard.key_down(&reports, hid::B).unwrap();
        assert_eq!(reports.last(), [0, 0, hid::B, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn rejected_release_and_release_all_remain_retryable() {
        let reports = Reports::default();
        let mut keyboard = KeyboardState::new();
        keyboard.key_down(&reports, hid::A).unwrap();
        reports.fail_on(&[2, 5]);
        assert!(keyboard.key_up(&reports, hid::A).is_err());
        keyboard.key_down(&reports, hid::B).unwrap();
        assert_eq!(reports.last(), [0, 0, hid::A, hid::B, 0, 0, 0, 0]);
        keyboard.key_up(&reports, hid::A).unwrap();
        assert!(keyboard.release_all(&reports).is_err());
        keyboard.key_down(&reports, hid::C).unwrap();
        assert_eq!(reports.last(), [0, 0, hid::B, hid::C, 0, 0, 0, 0]);
        keyboard.release_all(&reports).unwrap();
        assert_eq!(reports.last(), [0; 8]);
    }

    #[test]
    fn full_six_slots_do_not_leave_temporary_shift() {
        for ascii_only in [false, true] {
            let reports = Reports::default();
            let mut keyboard = KeyboardState::new();
            for usage in [hid::B, hid::C, hid::D, hid::E, hid::F, hid::G] {
                keyboard.key_down(&reports, usage).unwrap();
            }
            let result = keyboard.type_layout(&reports, "A", KeyboardLayout::Us, false, ascii_only);
            assert!(result.is_err());
            keyboard.key_down(&reports, hid::LEFT_CTRL).unwrap();
            assert_eq!(
                reports.last(),
                [1, 0, hid::B, hid::C, hid::D, hid::E, hid::F, hid::G]
            );
        }
    }

    #[test]
    fn combo_preserves_preexisting_keys_on_success_and_failure() {
        for fail in [None, Some(4)] {
            let reports = Reports::default();
            let mut keyboard = KeyboardState::new();
            keyboard.key_down(&reports, hid::LEFT_CTRL).unwrap();
            keyboard.key_down(&reports, hid::B).unwrap();
            if let Some(attempt) = fail {
                reports.fail_on(&[attempt]);
            }
            let result = keyboard.press_combo(&reports, &[hid::LEFT_CTRL, hid::A]);
            assert_eq!(result.is_err(), fail.is_some());
            keyboard.key_down(&reports, hid::C).unwrap();
            assert_eq!(reports.last(), [1, 0, hid::B, hid::C, 0, 0, 0, 0]);
        }
    }

    #[test]
    fn temporary_release_failure_is_cleaned_without_losing_old_keys() {
        let reports = Reports::default();
        let mut keyboard = KeyboardState::new();
        keyboard.key_down(&reports, hid::B).unwrap();
        reports.fail_on(&[3]);
        let error = keyboard.press(&reports, hid::A).unwrap_err();
        assert!(matches!(
            error,
            Error::Io { source, .. } if source.raw_os_error() == Some(3)
        ));
        keyboard.key_down(&reports, hid::C).unwrap();
        assert_eq!(reports.last(), [0, 0, hid::B, hid::C, 0, 0, 0, 0]);
    }

    #[test]
    fn layout_failure_does_not_release_preexisting_modifiers() {
        let reports = Reports::default();
        let mut keyboard = KeyboardState::new();
        keyboard.key_down(&reports, hid::LEFT_CTRL).unwrap();
        reports.fail_on(&[3]);
        assert!(keyboard
            .type_text_layout(&reports, "A", KeyboardLayout::Us)
            .is_err());
        keyboard.key_down(&reports, hid::B).unwrap();
        assert_eq!(reports.last(), [1, 0, hid::B, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn non_bmp_character_is_rejected_without_aliasing_a_bmp_key() {
        let reports = Reports::default();
        let mut keyboard = KeyboardState::new();
        assert!(keyboard
            .type_text_layout(&reports, "\u{10041}", KeyboardLayout::Us)
            .is_err());
        assert!(reports.accepted().is_empty());
    }

    #[test]
    fn ascii_and_layout_merge_crlf_but_keep_separate_newlines() {
        for ascii in [false, true] {
            let reports = Reports::default();
            let mut keyboard = KeyboardState::new();
            if ascii {
                keyboard.type_ascii(&reports, "\r\n\n\r").unwrap();
            } else {
                keyboard
                    .type_text_layout(&reports, "\r\n\n\r", KeyboardLayout::Us)
                    .unwrap();
            }
            let enters = reports
                .accepted()
                .iter()
                .filter(|(_, report)| report[2..].contains(&hid::ENTER))
                .count();
            assert_eq!(enters, 3);
        }
    }

    // Decode the observable HID reports through Windows, with fixed physical
    // key fixtures rather than reusing the production scan-to-HID mapping.
    fn decoded(reports: &Reports, hkl: isize, caps: bool) -> String {
        let mut text = String::new();
        for (_, report) in reports.accepted() {
            for usage in report[2..].iter().copied().filter(|usage| *usage != 0) {
                let scan = match usage {
                    0x04 => 0x1e,
                    0x14 => 0x10,
                    0x1c => 0x15,
                    0x1d => 0x2c,
                    0x1e => 0x02,
                    0x1f => 0x03,
                    0x20 => 0x04,
                    0x34 => 0x28,
                    0x08 => 0x12,
                    _ => panic!("unexpected text usage {usage:#x}"),
                };
                let vk = unsafe { MapVirtualKeyExW(scan, 3, hkl) };
                let mut state = [0u8; 256];
                state[0x14] = u8::from(caps);
                state[0x10] = if report[0] & 0x22 != 0 { 0x80 } else { 0 };
                state[0x11] = if report[0] & 0x11 != 0 { 0x80 } else { 0 };
                state[0x12] = if report[0] & 0x44 != 0 { 0x80 } else { 0 };
                let mut output = [0u16; 4];
                let count = unsafe {
                    ToUnicodeEx(vk, scan, state.as_ptr(), output.as_mut_ptr(), 4, 4, hkl)
                };
                assert_eq!(
                    count, 1,
                    "report must produce exactly one non-dead character"
                );
                text.push(char::from_u32(output[0] as u32).unwrap());
            }
        }
        text
    }

    #[test]
    fn caps_lock_text_matches_requested_characters_across_layouts() {
        for (layout, text) in [
            (KeyboardLayout::Us, "aA!"),
            (KeyboardLayout::Uk, "aA£"),
            (KeyboardLayout::De, "äÄzZ@"),
            (KeyboardLayout::Fr, "aAé€"),
        ] {
            let hkl = load_layout(layout).unwrap();
            for caps in [false, true] {
                let reports = Reports::default();
                let mut keyboard = KeyboardState::new();
                keyboard
                    .type_layout(&reports, text, layout, caps, false)
                    .unwrap();
                assert_eq!(
                    decoded(&reports, hkl, caps),
                    text,
                    "{layout:?}, caps={caps}"
                );
            }
        }
    }

    #[test]
    fn ascii_caps_lock_keeps_case_and_punctuation() {
        for caps in [false, true] {
            let reports = Reports::default();
            let mut keyboard = KeyboardState::new();
            keyboard
                .type_layout(&reports, "aA!", KeyboardLayout::Us, caps, true)
                .unwrap();
            assert_eq!(
                decoded(&reports, load_layout(KeyboardLayout::Us).unwrap(), caps),
                "aA!"
            );
        }
    }

    #[test]
    fn dead_key_is_rejected_without_sending_input() {
        let reports = Reports::default();
        let mut keyboard = KeyboardState::new();
        assert!(keyboard
            .type_layout(&reports, "^", KeyboardLayout::De, false, false)
            .is_err());
        assert!(reports.accepted().is_empty());
    }

    fn check_caps_snapshot_is_independent_of_the_callers_queue(ascii: bool) {
        #[link(name = "user32")]
        extern "system" {
            fn GetKeyboardState(state: *mut u8) -> i32;
            fn SetKeyboardState(state: *const u8) -> i32;
        }
        struct RestoreKeys([u8; 256]);
        impl Drop for RestoreKeys {
            fn drop(&mut self) {
                unsafe {
                    SetKeyboardState(self.0.as_ptr());
                }
            }
        }
        let mut saved = [0u8; 256];
        assert_ne!(unsafe { GetKeyboardState(saved.as_mut_ptr()) }, 0);
        let _restore = RestoreKeys(saved);
        let mut first = None;
        for caller_caps in [0, 1] {
            let mut local = saved;
            local[0x14] = caller_caps;
            assert_ne!(unsafe { SetKeyboardState(local.as_ptr()) }, 0);
            let reports = Reports::default();
            let mut keyboard = KeyboardState::new();
            if ascii {
                keyboard.type_ascii(&reports, "a").unwrap();
            } else {
                keyboard
                    .type_text_layout(&reports, "a", KeyboardLayout::Us)
                    .unwrap();
            }
            let current = reports.accepted();
            if let Some(expected) = &first {
                assert_eq!(
                    &current, expected,
                    "the caller's local queue must not change the emitted text"
                );
            } else {
                first = Some(current);
            }
        }
    }

    #[test]
    fn ascii_text_does_not_read_a_stale_caller_caps_state() {
        check_caps_snapshot_is_independent_of_the_callers_queue(true);
    }

    #[test]
    fn layout_text_does_not_read_a_stale_caller_caps_state() {
        check_caps_snapshot_is_independent_of_the_callers_queue(false);
    }
}
