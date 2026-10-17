# Daemon, Express Keys and Desktop Mode

**Task:** docs/TASKS.md #1
**Date:** 2026-10-17
**Status:** In Progress

## Summary
`decopad` is a background daemon that owns the Tablet's Express Keys and runs
the action configured for each one in the current mode. Express Key 1 toggles
between Desktop Mode, where the Pen drives the Pointer through the X wacom
driver, and Pad Mode, where decopad grabs the Pen so the Pointer stays put
(the Popup itself arrives in task 2).

## Decisions
- **D1 Pen in Desktop Mode:** release the Pen to the X wacom driver; grab it
  only in Pad Mode (docs/adr/0002).
- **D2 Shortcut output:** a uinput virtual keyboard, so shortcuts look like a
  real keyboard and survive a future Wayland move.
- **D3 Config:** TOML at `~/.config/decopad/config.toml`, one table per mode,
  reloaded automatically when saved. A commented default is written on first
  run.
- **D4 Name:** `decopad` for the binary, config folder and autostart entry.
- **Language and stack:** Rust with pure-Rust crates (docs/adr/0001).
- **Express Key detection:** Factory Mode combos are unique per key
  (docs/hardware-deco-640.md), so no Raw Mode command is needed.

## Affected Modules
New crate at `decopad/`:

| Module | Role |
|---|---|
| `device` | Find the Tablet's evdev nodes by USB ID 28bd:2904, grab and release them, recover from unplug and replug |
| `keys` | Decode Factory Mode key reports into Express Key 1–8 press and release |
| `config` | Parse the TOML config into per-mode actions; watch the file for changes |
| `actions` | uinput virtual keyboard; parse action strings like `ctrl+shift+z` |
| `mode` | Current mode; grab or release the Pen on toggle; end an in-progress stroke cleanly |
| `xmap` | Set the wacom driver's tablet area to the screen's aspect ratio |
| `main` | Event loop over the Tablet nodes, config watcher and hot-plug watcher |

## Implementation Plan
1. Scaffold the crate (`evdev`, `x11rb`, `serde`, `toml`, `inotify`, `anyhow`).
2. `device`: enumerate `/dev/input/event*` by USB ID and name suffix; grab the
   keyboard and mouse nodes permanently.
3. `keys`: match the held key set against the 8 known combos, emitting
   press on match and release when the combo's key is released.
4. `actions`: virtual keyboard; action grammar `mod+mod+key`, plus the
   built-in `mode-toggle` and `none`.
5. `config`: defaults (Desktop Mode keeps today's factory shortcuts, Pad Mode
   per the design), load, validate with clear errors, reload on save while
   keeping the previous config if the new one is invalid.
6. `mode`: Pad Mode grabs the Pen node; Desktop Mode releases it. Toggling is
   deferred until the tip lifts if pressed mid-stroke. Desktop notification on
   each switch.
7. `xmap`: on start, set `Wacom Tablet Area` so the Active Area matches the
   screen's aspect ratio.
8. Hot-plug: watch `/dev/input` with inotify; reattach when the Tablet returns.
9. Autostart: `~/.config/autostart/decopad.desktop`; install with
   `cargo install --path decopad`.
10. Unit tests for combo decoding, action parsing, config parsing and area
    maths; manual verification on the real Tablet.
