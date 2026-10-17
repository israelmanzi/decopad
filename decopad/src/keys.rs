//! Turns the Tablet's Factory Mode keyboard combos into Express Key presses.
//!
//! In Factory Mode each Express Key types a fixed combo on the Tablet's own
//! keyboard node (see docs/hardware-deco-640.md). Every combo is distinct, so
//! the combo identifies the key.

use evdev::KeyCode;

const CTRL: u8 = 1;
const SHIFT: u8 = 2;
const ALT: u8 = 4;
const META: u8 = 8;

/// Factory Mode combo of Express Keys 1–8, in key order.
const COMBOS: [(u8, KeyCode); 8] = [
    (0, KeyCode::KEY_B),
    (0, KeyCode::KEY_E),
    (0, KeyCode::KEY_LEFTBRACE),
    (0, KeyCode::KEY_RIGHTBRACE),
    (CTRL, KeyCode::KEY_KPMINUS),
    (CTRL, KeyCode::KEY_KPPLUS),
    (CTRL, KeyCode::KEY_Z),
    (CTRL | SHIFT, KeyCode::KEY_Z),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExpressKey {
    /// Express Key 1–8 went down.
    Pressed(usize),
    /// Express Key 1–8 came back up.
    Released(usize),
}

fn modifier_bit(code: KeyCode) -> Option<u8> {
    match code {
        KeyCode::KEY_LEFTCTRL | KeyCode::KEY_RIGHTCTRL => Some(CTRL),
        KeyCode::KEY_LEFTSHIFT | KeyCode::KEY_RIGHTSHIFT => Some(SHIFT),
        KeyCode::KEY_LEFTALT | KeyCode::KEY_RIGHTALT => Some(ALT),
        KeyCode::KEY_LEFTMETA | KeyCode::KEY_RIGHTMETA => Some(META),
        _ => None,
    }
}

#[derive(Default)]
pub struct Decoder {
    modifiers: u8,
    /// The Express Key currently down and the combo key that identifies it.
    active: Option<(usize, KeyCode)>,
}

impl Decoder {
    /// Feeds one synchronised report of key changes `(code, value)`.
    ///
    /// Modifiers are applied before other keys so a combo whose modifier and
    /// key share a report is matched regardless of their order in it.
    pub fn feed(&mut self, report: &[(KeyCode, i32)]) -> Vec<ExpressKey> {
        for &(code, value) in report {
            if let Some(bit) = modifier_bit(code) {
                match value {
                    0 => self.modifiers &= !bit,
                    _ => self.modifiers |= bit,
                }
            }
        }
        let mut out = Vec::new();
        for &(code, value) in report {
            if modifier_bit(code).is_some() {
                continue;
            }
            match value {
                1 => {
                    let found = COMBOS
                        .iter()
                        .position(|&(mods, key)| key == code && mods == self.modifiers);
                    if let Some(index) = found {
                        if let Some((previous, _)) = self.active.take() {
                            out.push(ExpressKey::Released(previous));
                        }
                        self.active = Some((index + 1, code));
                        out.push(ExpressKey::Pressed(index + 1));
                    }
                }
                0 => {
                    if let Some((key, held)) = self.active
                        && held == code
                    {
                        self.active = None;
                        out.push(ExpressKey::Released(key));
                    }
                }
                // Autorepeat while the key is held is not a new press.
                _ => {}
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ExpressKey::*;

    fn press_release(press: &[(KeyCode, i32)], release: &[(KeyCode, i32)]) -> Vec<ExpressKey> {
        let mut decoder = Decoder::default();
        let mut out = decoder.feed(press);
        out.extend(decoder.feed(release));
        out
    }

    #[test]
    fn plain_key_is_express_key_1() {
        let out = press_release(&[(KeyCode::KEY_B, 1)], &[(KeyCode::KEY_B, 0)]);
        assert_eq!(out, [Pressed(1), Released(1)]);
    }

    #[test]
    fn shift_distinguishes_key_8_from_key_7() {
        let press = [
            (KeyCode::KEY_LEFTCTRL, 1),
            (KeyCode::KEY_LEFTSHIFT, 1),
            (KeyCode::KEY_Z, 1),
        ];
        let release = [
            (KeyCode::KEY_LEFTCTRL, 0),
            (KeyCode::KEY_LEFTSHIFT, 0),
            (KeyCode::KEY_Z, 0),
        ];
        assert_eq!(press_release(&press, &release), [Pressed(8), Released(8)]);

        let press = [(KeyCode::KEY_LEFTCTRL, 1), (KeyCode::KEY_Z, 1)];
        let release = [(KeyCode::KEY_LEFTCTRL, 0), (KeyCode::KEY_Z, 0)];
        assert_eq!(press_release(&press, &release), [Pressed(7), Released(7)]);
    }

    #[test]
    fn modifier_after_key_in_same_report_still_matches() {
        let press = [(KeyCode::KEY_KPPLUS, 1), (KeyCode::KEY_LEFTCTRL, 1)];
        let release = [(KeyCode::KEY_KPPLUS, 0), (KeyCode::KEY_LEFTCTRL, 0)];
        assert_eq!(press_release(&press, &release), [Pressed(6), Released(6)]);
    }

    #[test]
    fn autorepeat_and_unknown_keys_are_ignored() {
        let mut decoder = Decoder::default();
        assert_eq!(decoder.feed(&[(KeyCode::KEY_E, 1)]), [Pressed(2)]);
        assert_eq!(decoder.feed(&[(KeyCode::KEY_E, 2)]), []);
        assert_eq!(decoder.feed(&[(KeyCode::KEY_Q, 1)]), []);
        assert_eq!(decoder.feed(&[(KeyCode::KEY_E, 0)]), [Released(2)]);
    }

    #[test]
    fn unmodified_z_is_not_an_express_key() {
        let out = press_release(&[(KeyCode::KEY_Z, 1)], &[(KeyCode::KEY_Z, 0)]);
        assert_eq!(out, []);
    }
}
