# Changelog

## 2026-10-17

### Added
- decopad daemon for the XP-Pen Deco 640 (Rust, X11): Express Keys decoded
  from their factory shortcuts and remapped per mode from
  `~/.config/decopad/config.toml`, with live reload, hot-plug recovery,
  single instance and autostart.
- Writing pad (Pad Mode): an always-on-top, focus-free pad that is
  movable and resizable with the mouse, with ruled lines, a pen-tip ring and
  a three-line legend.
- Commit: replays pad ink at the pointer through a virtual pen; new line;
  continuing the line after each Commit; several lines per Commit.
- Picture Paste: the ink as a PNG on the clipboard, pasted with Ctrl+V.
- Live blue preview and caret showing where ink will land.
- Undo and redo on keys 7/8 and the pen buttons: the pad's strokes first,
  then the app. Hold the upper pen button to erase.
- Ink size in 10% steps, remembered between runs.
- `decopad --default-config`.
- Tools: `tools/capture.py`, `tools/e2e.py` (simulated-tablet end-to-end
  suite), `tools/bench.py`; `setup/install.sh`.

### Changed
- Normalized controls: the same key position means the same idea in both
  modes, and Pad Mode means "pad open".
- Esc closes the pad and keeps its ink.
- Mode and reload notifications removed; only config errors notify.

### Performance
- Incremental preview: writing CPU stays at about 1.5% instead of climbing
  to 15%.
- Stroke simplification and a 2 ms replay interval: Commit is about 4×
  faster.
- Release build with LTO and stripping: 1.6 MB.
