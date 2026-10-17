//! What an Express Key or Pen Button does, and the virtual keyboard that
//! sends shortcuts.

use std::str::FromStr;

use anyhow::{Context, Result, bail};
use evdev::uinput::VirtualDevice;
use evdev::{AttributeSet, InputEvent, KeyCode, KeyEvent, RelativeAxisCode};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Action {
    #[default]
    None,
    /// Open the Popup (Pad Mode) or close it (Desktop Mode).
    TogglePad,
    /// Keys pressed in order while the Express Key is held, released in reverse.
    Shortcut(Vec<KeyCode>),
    Builtin(Builtin),
}

/// decopad's own actions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Builtin {
    Commit,
    /// Commit, then continue on the next line.
    NewLine,
    PicturePaste,
    /// Undo the last stroke in the Popup, or Ctrl+Z in the app when there is
    /// nothing to undo there.
    Undo,
    /// Redo a stroke undone in the Popup, or Ctrl+Y in the app when there is
    /// nothing to redo there.
    Redo,
    InkSmaller,
    InkLarger,
}

impl FromStr for Action {
    type Err = anyhow::Error;

    fn from_str(text: &str) -> Result<Self> {
        let text = text.trim().to_lowercase();
        Ok(match text.as_str() {
            "none" => Action::None,
            "toggle-pad" | "mode-toggle" => Action::TogglePad,
            "commit" => Action::Builtin(Builtin::Commit),
            "new-line" => Action::Builtin(Builtin::NewLine),
            "picture-paste" => Action::Builtin(Builtin::PicturePaste),
            "undo" => Action::Builtin(Builtin::Undo),
            "redo" => Action::Builtin(Builtin::Redo),
            "ink-smaller" => Action::Builtin(Builtin::InkSmaller),
            "ink-larger" => Action::Builtin(Builtin::InkLarger),
            "right-click" => Action::Shortcut(vec![KeyCode::BTN_RIGHT]),
            _ => Action::Shortcut(parse_shortcut(&text)?),
        })
    }
}

fn parse_shortcut(text: &str) -> Result<Vec<KeyCode>> {
    if text.is_empty() {
        bail!("empty action");
    }
    text.split('+').map(parse_key).collect()
}

fn parse_key(name: &str) -> Result<KeyCode> {
    let code = match name.trim() {
        "ctrl" => KeyCode::KEY_LEFTCTRL,
        "shift" => KeyCode::KEY_LEFTSHIFT,
        "alt" => KeyCode::KEY_LEFTALT,
        "altgr" => KeyCode::KEY_RIGHTALT,
        "super" => KeyCode::KEY_LEFTMETA,
        "[" => KeyCode::KEY_LEFTBRACE,
        "]" => KeyCode::KEY_RIGHTBRACE,
        "-" => KeyCode::KEY_MINUS,
        "=" => KeyCode::KEY_EQUAL,
        ";" => KeyCode::KEY_SEMICOLON,
        "'" => KeyCode::KEY_APOSTROPHE,
        "," => KeyCode::KEY_COMMA,
        "." => KeyCode::KEY_DOT,
        "/" => KeyCode::KEY_SLASH,
        "\\" => KeyCode::KEY_BACKSLASH,
        "`" => KeyCode::KEY_GRAVE,
        "" => bail!("missing key between '+' signs"),
        other => {
            let upper = other.to_uppercase().replace('-', "");
            let evdev_name = match upper.strip_prefix("KEY_") {
                Some(_) => upper.clone(),
                None => format!("KEY_{upper}"),
            };
            KeyCode::from_str(&evdev_name)
                .map_err(|_| anyhow::anyhow!("unknown key \"{other}\""))?
        }
    };
    Ok(code)
}

/// A uinput keyboard that types shortcuts as if from a real keyboard. It
/// also has mouse buttons, for "right-click".
pub struct VirtualKeyboard {
    device: VirtualDevice,
}

impl VirtualKeyboard {
    pub fn new() -> Result<Self> {
        let mut keys = AttributeSet::<KeyCode>::new();
        // Every ordinary keyboard key, from KEY_ESC up to KEY_MICMUTE.
        for code in 1..=248 {
            keys.insert(KeyCode::new(code));
        }
        for button in [KeyCode::BTN_LEFT, KeyCode::BTN_RIGHT, KeyCode::BTN_MIDDLE] {
            keys.insert(button);
        }
        // X only treats a device with motion axes as a mouse.
        let mut axes = AttributeSet::<RelativeAxisCode>::new();
        axes.insert(RelativeAxisCode::REL_X);
        axes.insert(RelativeAxisCode::REL_Y);
        let device = VirtualDevice::builder()
            .context("cannot open /dev/uinput (is the uinput module loaded?)")?
            .name("decopad keyboard")
            .with_keys(&keys)?
            .with_relative_axes(&axes)?
            .build()?;
        Ok(Self { device })
    }

    pub fn press(&mut self, keys: &[KeyCode]) -> Result<()> {
        let events: Vec<InputEvent> = keys.iter().map(|&k| *KeyEvent::new(k, 1)).collect();
        self.device.emit(&events)?;
        Ok(())
    }

    pub fn release(&mut self, keys: &[KeyCode]) -> Result<()> {
        let events: Vec<InputEvent> = keys.iter().rev().map(|&k| *KeyEvent::new(k, 0)).collect();
        self.device.emit(&events)?;
        Ok(())
    }
}

impl Action {
    /// What the action does, for the Cheatsheet and notifications; `None`
    /// for keys that do nothing. `pad_open` says whether the Popup is open,
    /// which decides what toggling it means.
    pub fn label(&self, pad_open: bool) -> Option<String> {
        let label = match self {
            Action::None => return None,
            Action::TogglePad if pad_open => "Close",
            Action::TogglePad => "Writing pad",
            Action::Shortcut(keys) => return Some(shortcut_label(keys)),
            Action::Builtin(Builtin::Commit) => "Commit",
            Action::Builtin(Builtin::NewLine) => "New line",
            Action::Builtin(Builtin::PicturePaste) => "Paste as picture",
            Action::Builtin(Builtin::Undo) => "Undo",
            Action::Builtin(Builtin::Redo) => "Redo",
            Action::Builtin(Builtin::InkSmaller) => "Smaller ink",
            Action::Builtin(Builtin::InkLarger) => "Larger ink",
        };
        Some(label.into())
    }
}

/// Names common shortcuts by what they do; others by their keys.
fn shortcut_label(keys: &[KeyCode]) -> String {
    use KeyCode as K;
    let named = match keys {
        [K::KEY_LEFTCTRL, K::KEY_C] => "Copy",
        [K::KEY_LEFTCTRL, K::KEY_X] => "Cut",
        [K::KEY_LEFTCTRL, K::KEY_V] => "Paste",
        [K::KEY_LEFTCTRL, K::KEY_Z] => "Undo",
        [K::KEY_LEFTCTRL, K::KEY_Y] | [K::KEY_LEFTCTRL, K::KEY_LEFTSHIFT, K::KEY_Z] => "Redo",
        [K::KEY_LEFTCTRL, K::KEY_S] => "Save",
        [K::KEY_LEFTCTRL, K::KEY_A] => "Select all",
        [K::KEY_LEFTCTRL, K::KEY_KPMINUS] | [K::KEY_LEFTCTRL, K::KEY_MINUS] => "Zoom out",
        [K::KEY_LEFTCTRL, K::KEY_KPPLUS] | [K::KEY_LEFTCTRL, K::KEY_EQUAL] => "Zoom in",
        [K::BTN_RIGHT] => "Right click",
        _ => {
            return keys
                .iter()
                .map(|&k| key_label(k))
                .collect::<Vec<_>>()
                .join("+");
        }
    };
    named.into()
}

fn key_label(code: KeyCode) -> String {
    let label = match code {
        KeyCode::KEY_LEFTCTRL | KeyCode::KEY_RIGHTCTRL => "Ctrl",
        KeyCode::KEY_LEFTSHIFT | KeyCode::KEY_RIGHTSHIFT => "Shift",
        KeyCode::KEY_LEFTALT => "Alt",
        KeyCode::KEY_RIGHTALT => "AltGr",
        KeyCode::KEY_LEFTMETA | KeyCode::KEY_RIGHTMETA => "Super",
        KeyCode::KEY_LEFTBRACE => "[",
        KeyCode::KEY_RIGHTBRACE => "]",
        KeyCode::KEY_MINUS => "-",
        KeyCode::KEY_EQUAL => "=",
        KeyCode::KEY_KPMINUS => "Num-",
        KeyCode::KEY_KPPLUS => "Num+",
        _ => {
            let name = format!("{code:?}");
            let name = name.trim_start_matches("KEY_");
            let mut chars = name.chars();
            return match chars.next() {
                Some(first) => first.to_string() + &chars.as_str().to_lowercase(),
                None => name.to_string(),
            };
        }
    };
    label.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn label(text: &str) -> Option<String> {
        text.parse::<Action>().unwrap().label(true)
    }

    #[test]
    fn labels_say_what_keys_do() {
        assert_eq!(label("ctrl+shift+z").as_deref(), Some("Redo"));
        assert_eq!(label("ctrl+kp-minus").as_deref(), Some("Zoom out"));
        assert_eq!(label("ctrl+v").as_deref(), Some("Paste"));
        assert_eq!(label("right-click").as_deref(), Some("Right click"));
        assert_eq!(label("ctrl+alt+t").as_deref(), Some("Ctrl+Alt+T"));
        assert_eq!(label("page-up").as_deref(), Some("Pageup"));
        assert_eq!(label("picture-paste").as_deref(), Some("Paste as picture"));
        assert_eq!(label("none"), None);
        let toggle = "toggle-pad".parse::<Action>().unwrap();
        assert_eq!(toggle.label(true).as_deref(), Some("Close"));
        assert_eq!(toggle.label(false).as_deref(), Some("Writing pad"));
    }

    fn shortcut(text: &str) -> Vec<KeyCode> {
        match text.parse::<Action>().unwrap() {
            Action::Shortcut(keys) => keys,
            other => panic!("{text} parsed as {other:?}"),
        }
    }

    #[test]
    fn modifiers_and_named_keys() {
        assert_eq!(
            shortcut("Ctrl+Shift+Z"),
            [
                KeyCode::KEY_LEFTCTRL,
                KeyCode::KEY_LEFTSHIFT,
                KeyCode::KEY_Z
            ]
        );
        assert_eq!(
            shortcut("ctrl+kp-minus"),
            [KeyCode::KEY_LEFTCTRL, KeyCode::KEY_KPMINUS]
        );
        assert_eq!(shortcut("page-up"), [KeyCode::KEY_PAGEUP]);
        assert_eq!(
            shortcut("super+f5"),
            [KeyCode::KEY_LEFTMETA, KeyCode::KEY_F5]
        );
        assert_eq!(shortcut("["), [KeyCode::KEY_LEFTBRACE]);
        assert_eq!(shortcut("KEY_VOLUMEUP"), [KeyCode::KEY_VOLUMEUP]);
    }

    #[test]
    fn built_in_actions() {
        assert_eq!("toggle-pad".parse::<Action>().unwrap(), Action::TogglePad);
        assert_eq!("mode-toggle".parse::<Action>().unwrap(), Action::TogglePad);
        assert_eq!(" none ".parse::<Action>().unwrap(), Action::None);
        assert_eq!(
            "commit".parse::<Action>().unwrap(),
            Action::Builtin(Builtin::Commit)
        );
    }

    #[test]
    fn bad_shortcuts_explain_themselves() {
        let error = "ctrl+banana".parse::<Action>().unwrap_err().to_string();
        assert!(error.contains("banana"), "{error}");
        assert!("ctrl++".parse::<Action>().is_err());
        assert!("".parse::<Action>().is_err());
        assert!("undo-stroke".parse::<Action>().is_err(), "replaced by undo");
    }
}
