//! The user's config file: what each Express Key and Pen Button does in each
//! mode, and how the Popup behaves. Also the remembered Ink Scale.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::actions::Action;
use crate::mode::Mode;

pub const FILE_NAME: &str = "config.toml";

/// Written on first run; also the reference for every option.
pub const DEFAULT: &str = r#"# decopad configuration. Changes apply as soon as you save this file.
#
# decopad has two modes:
#   Desktop Mode  the Writing pad is closed; the Pen is a normal tablet pen
#   Pad Mode      the Writing pad is open; the Pen writes into it and the
#                 Pointer stays where your writing will go
# Express Key 1 or Super+W switches between them.
#
# key1..key8 are the Express Keys, numbered by the shortcut each one typed
# before decopad: key1 = B, key2 = E, key3 = [, key4 = ], key5 = Ctrl+-,
# key6 = Ctrl++, key7 = Ctrl+Z, key8 = Ctrl+Shift+Z.
# pen-button1 is the Pen's lower side button, pen-button2 the upper one.
# In Desktop Mode, "none" on a pen button leaves it to X (middle/right click).
# pen-button1-hold / pen-button2-hold = "eraser" makes holding the button
# while the Pen touches the Tablet erase; the button's own action then runs
# when it is clicked without touching. In Desktop Mode this uses the app's
# eraser, which works only for the button the pen reports as its eraser
# (the upper one on the Deco 640).
#
# An action is one of:
#   "ctrl+shift+z"  a shortcut: modifiers (ctrl, shift, alt, altgr, super)
#                   joined with "+" and ending in a key such as a-z, 0-9,
#                   f1-f24, [ ] - = ; ' , . / \ `, space, enter, tab, esc,
#                   backspace, delete, home, end, page-up, page-down, up,
#                   down, left, right, kp-minus, kp-plus, or any evdev key
#                   name like KEY_VOLUMEUP. It is held while the key is held.
#   "right-click"   a right mouse click where the Pointer is
#   "toggle-pad"    open or close the Writing pad
#   "undo", "redo"  undo or redo a stroke in the Writing pad; with nothing to
#                   undo or redo there, Ctrl+Z or Ctrl+Y in the app
#   "none"          do nothing
# In Pad Mode also:
#   "commit"        write the pad's Ink at the Pointer
#   "new-line"      commit, then continue on the next line
#   "picture-paste" paste the Ink as an image where the app pastes
#   "ink-smaller", "ink-larger"   change the size of the written Ink
#
# Keyboard while writing (Pen near the Tablet, or Ink not yet committed):
# Enter = new line, Backspace = clear, Tab = eraser, Esc = close the pad.
# Drag the pad with the mouse to move it, or by its bottom-right corner to
# resize it; decopad remembers both.

# Mode at startup: "desktop" or "pad".
start-mode = "desktop"

# Writing pad closed: the Pen moves the Pointer.
[desktop]
key1 = "toggle-pad"
key2 = "right-click"
key3 = "ctrl+v"
key4 = "enter"
key5 = "ctrl+kp-minus"
key6 = "ctrl+kp-plus"
key7 = "undo"
key8 = "redo"
pen-button1 = "undo"
pen-button2 = "redo"
pen-button2-hold = "eraser"

# Writing pad open: the same positions do the matching pad actions.
[pad]
key1 = "toggle-pad"
key2 = "commit"
key3 = "picture-paste"
key4 = "new-line"
key5 = "ink-smaller"
key6 = "ink-larger"
key7 = "undo"
key8 = "redo"
pen-button1 = "undo"
pen-button2 = "redo"
pen-button2-hold = "eraser"

[popup]
# Writing pad width in pixels until you resize it; the height follows the
# Tablet's shape.
width = 960
# Starting Ink size: screen pixels per millimetre written (3.0 shows as 100%).
# Keys 5 and 6 adjust it while writing, and decopad remembers the last value.
ink-scale = 3.0
# Commit automatically once the Pen has been away from the Tablet this long
# (milliseconds), e.g. 1500. 0 commits only on a key, so you can write
# several lines before committing them together.
commit-delay-ms = 0
# Distance between the pad's ruled lines, in millimetres on the Tablet.
line-spacing-mm = 15.0
# Pause between replayed points, in milliseconds. 1 is the fastest that
# apps reproduce exactly (0 makes them merge strokes); raise it if an app
# drops parts of strokes.
replay-interval-ms = 1
"#;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Table {
    pub keys: [Action; 8],
    /// What a click of each Pen Button does.
    pub pen_buttons: [Action; 2],
    /// What holding each Pen Button does while the Pen touches the Tablet.
    pub pen_holds: [Hold; 2],
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Hold {
    /// The button's action happens on press, and a shortcut stays held.
    #[default]
    None,
    /// Erases while held; the button's action happens on release instead,
    /// and only if the Pen did not touch the Tablet in between.
    Eraser,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case", default)]
pub struct PopupConfig {
    pub width: u32,
    pub ink_scale: f64,
    pub commit_delay_ms: u64,
    pub line_spacing_mm: f64,
    pub replay_interval_ms: u64,
}

impl Default for PopupConfig {
    fn default() -> Self {
        PopupConfig {
            width: 960,
            ink_scale: 3.0,
            commit_delay_ms: 0,
            line_spacing_mm: 15.0,
            replay_interval_ms: 1,
        }
    }
}

pub const INK_SCALE_RANGE: (f64, f64) = (1.0, 12.0);

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub start_mode: Mode,
    pub desktop: Table,
    pub pad: Table,
    pub popup: PopupConfig,
}

impl Config {
    pub fn table(&self, mode: Mode) -> &Table {
        match mode {
            Mode::Desktop => &self.desktop,
            Mode::Pad => &self.pad,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct RawConfig {
    #[serde(default)]
    start_mode: Mode,
    #[serde(default)]
    desktop: BTreeMap<String, String>,
    #[serde(default)]
    pad: BTreeMap<String, String>,
    #[serde(default)]
    popup: PopupConfig,
}

pub fn parse(text: &str) -> Result<Config> {
    let raw: RawConfig = toml::from_str(text)?;
    let popup = raw.popup;
    let check = |ok: bool, what: &str| {
        if ok {
            Ok(())
        } else {
            Err(anyhow::anyhow!("[popup] {what}"))
        }
    };
    check(
        (320..=2560).contains(&popup.width),
        "width must be 320 to 2560",
    )?;
    check(
        (INK_SCALE_RANGE.0..=INK_SCALE_RANGE.1).contains(&popup.ink_scale),
        "ink-scale must be 1 to 12",
    )?;
    check(
        (5.0..=45.0).contains(&popup.line_spacing_mm),
        "line-spacing-mm must be 5 to 45",
    )?;
    check(
        (1..=50).contains(&popup.replay_interval_ms),
        "replay-interval-ms must be 1 to 50",
    )?;
    Ok(Config {
        start_mode: raw.start_mode,
        desktop: parse_table("desktop", &raw.desktop)?,
        pad: parse_table("pad", &raw.pad)?,
        popup,
    })
}

/// Entries left out of a table do nothing.
fn parse_table(table: &str, raw: &BTreeMap<String, String>) -> Result<Table> {
    let mut out = Table::default();
    for (name, text) in raw {
        let hold = match name.as_str() {
            "pen-button1-hold" => Some(0),
            "pen-button2-hold" => Some(1),
            _ => None,
        };
        if let Some(i) = hold {
            out.pen_holds[i] = match text.trim().to_lowercase().as_str() {
                "eraser" => Hold::Eraser,
                "none" => Hold::None,
                _ => bail!("[{table}] {name} = \"{text}\": expected \"eraser\" or \"none\""),
            };
            continue;
        }
        let slot = match name.strip_prefix("pen-button") {
            Some("1") => &mut out.pen_buttons[0],
            Some("2") => &mut out.pen_buttons[1],
            _ => {
                let index = name
                    .strip_prefix("key")
                    .and_then(|n| n.parse::<usize>().ok())
                    .filter(|n| (1..=8).contains(n));
                let Some(index) = index else {
                    bail!("[{table}] {name}: expected key1 to key8 or pen-button1/2[-hold]");
                };
                &mut out.keys[index - 1]
            }
        };
        *slot = text
            .parse()
            .with_context(|| format!("[{table}] {name} = \"{text}\""))?;
    }
    Ok(out)
}

fn xdg_dir(variable: &str, fallback: &str) -> Result<PathBuf> {
    let base = match std::env::var_os(variable) {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => PathBuf::from(std::env::var_os("HOME").context("HOME is not set")?).join(fallback),
    };
    Ok(base.join("decopad"))
}

pub fn dir() -> Result<PathBuf> {
    xdg_dir("XDG_CONFIG_HOME", ".config")
}

/// Loads the config, writing the default first if there is none.
pub fn load_or_create(path: &Path) -> Result<Config> {
    if !path.exists() {
        fs::create_dir_all(path.parent().unwrap())?;
        fs::write(path, DEFAULT)?;
    }
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    parse(&text).with_context(|| format!("in {}", path.display()))
}

/// Values decopad remembers between runs, in `~/.local/state/decopad/NAME`.
fn read_state(name: &str) -> Option<String> {
    fs::read_to_string(xdg_dir("XDG_STATE_HOME", ".local/state").ok()?.join(name)).ok()
}

fn write_state(name: &str, text: &str) {
    let saved = xdg_dir("XDG_STATE_HOME", ".local/state").and_then(|dir| {
        fs::create_dir_all(&dir)?;
        fs::write(dir.join(name), text)?;
        Ok(())
    });
    if let Err(error) = saved {
        eprintln!("decopad: cannot remember {name}: {error:#}");
    }
}

/// The Ink Scale last set with keys 5 and 6, if any.
pub fn remembered_ink_scale() -> Option<f64> {
    let scale: f64 = read_state("ink-scale")?.trim().parse().ok()?;
    (INK_SCALE_RANGE.0..=INK_SCALE_RANGE.1)
        .contains(&scale)
        .then_some(scale)
}

pub fn remember_ink_scale(scale: f64) {
    write_state("ink-scale", &format!("{scale}\n"));
}

/// Where the user last moved the Popup, and its width.
pub fn remembered_popup() -> Option<((i32, i32), u32)> {
    let text = read_state("popup")?;
    let mut numbers = text.split_whitespace().map(str::parse::<i64>);
    let (x, y, width) = (
        numbers.next()?.ok()?,
        numbers.next()?.ok()?,
        numbers.next()?.ok()?,
    );
    Some(((x as i32, y as i32), width.clamp(320, 2560) as u32))
}

pub fn remember_popup(position: (i32, i32), width: u32) {
    write_state("popup", &format!("{} {} {width}\n", position.0, position.1));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::Builtin;
    use evdev::KeyCode;

    #[test]
    fn default_config_mirrors_both_modes() {
        let config = parse(DEFAULT).unwrap();
        assert_eq!(config.start_mode, Mode::Desktop);
        assert_eq!(config.desktop.keys[0], Action::TogglePad);
        assert_eq!(config.pad.keys[0], Action::TogglePad);
        assert_eq!(
            config.desktop.keys[2],
            Action::Shortcut(vec![KeyCode::KEY_LEFTCTRL, KeyCode::KEY_V])
        );
        // Undo and redo sit in the same places in both modes.
        for table in [&config.desktop, &config.pad] {
            assert_eq!(table.keys[6], Action::Builtin(Builtin::Undo));
            assert_eq!(table.keys[7], Action::Builtin(Builtin::Redo));
            assert_eq!(
                table.pen_buttons,
                [
                    Action::Builtin(Builtin::Undo),
                    Action::Builtin(Builtin::Redo)
                ]
            );
            assert_eq!(table.pen_holds, [Hold::None, Hold::Eraser]);
        }
        assert_eq!(config.popup, PopupConfig::default());
    }

    #[test]
    fn missing_entries_and_tables_use_defaults() {
        let config = parse("[desktop]\nkey3 = \"ctrl+c\"\n").unwrap();
        assert_eq!(config.desktop.keys[0], Action::None);
        assert_eq!(config.pad, Table::default());
        let config = parse("[popup]\nwidth = 800\n").unwrap();
        assert_eq!(config.popup.width, 800);
        assert_eq!(config.popup.ink_scale, 3.0);
    }

    #[test]
    fn errors_name_the_offending_line() {
        let error = format!("{:#}", parse("[pad]\nkey9 = \"a\"\n").unwrap_err());
        assert!(error.contains("[pad] key9"), "{error}");
        let error = format!(
            "{:#}",
            parse("[desktop]\nkey2 = \"ctrl+nope\"\n").unwrap_err()
        );
        assert!(error.contains("key2") && error.contains("nope"), "{error}");
        let error = format!("{:#}", parse("[popup]\nink-scale = 40.0\n").unwrap_err());
        assert!(error.contains("ink-scale"), "{error}");
        assert!(parse("colour = \"blue\"\n").is_err());
        assert!(parse("[popup]\nheight = 3\n").is_err());
        let error = format!(
            "{:#}",
            parse("[pad]\npen-button2-hold = \"redo\"\n").unwrap_err()
        );
        assert!(error.contains("eraser"), "{error}");
    }
}
