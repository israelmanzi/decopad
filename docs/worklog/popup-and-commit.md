# Popup, Replay and Picture Paste

**Task:** docs/TASKS.md #2 and #3
**Date:** 2026-10-17
**Status:** Done

## Summary
In Pad Mode the Pen writes into the Popup, a small always-on-top window that
never takes keyboard focus, so the target app stays focused. Commit replays
the Ink at the Anchor through a virtual pen and keeps the Popup open; Picture
Paste puts the Ink on the clipboard as a PNG and presses Ctrl+V. The user
asked for tasks 2 and 3 to be built together and tested as a whole.

## Decisions
- **Popup never takes focus:** an override-redirect X window without
  decorations. Super+W is grabbed permanently; Esc, Backspace, Tab and Enter
  only while Writing (Pen in range or uncommitted Ink), so typing in other
  apps works while the Popup stays open.
- **Popup stays open after Commit** (user decision, revised 2026-10-17). It
  moves when dragged with the mouse and resizes from its bottom-right corner
  at the Tablet's aspect ratio; position and width are remembered in
  `~/.local/state/decopad/popup`.
- **Replay under the Popup:** if the replayed strokes or the final Pointer
  position overlap the Popup, it is unmapped for the Replay and mapped again
  afterwards, since X would deliver the virtual pen's input to it.
- **Vertical placement:** the baseline of the ruled line where the first stroke
  starts lands on the Anchor, so writing on any line puts it at the Pointer.
  Horizontally, the Ink's left edge lands on the Anchor.
- **Anchor at Commit time:** the Pointer position when committing. After a
  Replay the Pointer is warped to the right of the delivered Ink plus a 4 mm
  word gap (at Ink Scale). Enter commits and moves to the next line, back to
  where the line started. Moving the Pointer starts a new line origin.
- **Replay device:** a uinput tablet `decopad pen` with one axis unit per
  1/8 screen pixel. The X wacom driver averages 4 samples by default, so
  decopad sets `Wacom Sample and Suppress` to `0 1` on it; a probe showed
  pixel-exact placement with that setting and up to 75% lag without it.
- **Replay pacing:** points closer than 0.75 px are dropped and the rest are
  sent every `replay-interval-ms` (default 4 ms) so toolkits that compress
  motion events still see every point.
- **Eraser:** stroke eraser (removes whole strokes it touches), toggled with
  Tab or used automatically by the Pen's eraser end.
- **Ink Scale:** ±0.25 px/mm per press of keys 5/6, from 1 to 12, remembered
  in `~/.local/state/decopad/ink-scale`.
- **Picture Paste:** decopad owns CLIPBOARD with `image/png` and replaces
  whatever was there. Ctrl+V is pressed 250 ms after taking ownership, because
  apps such as Xournal++ enable Paste only after querying the new owner, and
  an immediate Ctrl+V lost that race whenever the app was busy (always right
  after a Replay). Where the picture lands is the app's choice; Xournal++
  centres it in the view.

- **Several lines per Commit** (user request): the Popup's ruled lines keep
  their layout on Replay, so the pause Commit now defaults to off
  (`commit-delay-ms = 0`) to avoid committing between lines.
- **Preview** (user request, "can't see where I'm writing"): a caret at the
  Pointer and the Ink in blue at its destination, as X Shape windows with an
  empty input region. Updated at most every 40 ms while Ink changes; the
  Pointer is polled every 100 ms while the Popup is open.
- **Pen Buttons** (user request): undo/redo. Pad Mode `undo`/`redo` act on the
  Popup's strokes, falling back to Ctrl+Z/Ctrl+Y in the app when there is
  nothing to undo or redo there. Desktop Mode sends Ctrl+Z/Ctrl+Y; decopad
  reads the buttons from the ungrabbed Pen and sets X buttons 2 and 3 to 0 with
  `xsetwacom` so they do not also click. The Deco 640's upper button reports
  the eraser tool (docs/hardware-deco-640.md), so it is read as Pen Button 2,
  and X's `… Pen eraser` device is floated so apps never see an eraser.
  Disabling that device was tried first and also silenced the stylus. These
  X settings stay if decopad exits, until the Tablet is replugged.

- **Normalized controls** (user request: conflicting keys and a confusing
  legend, permission to revise earlier choices):
  - Pad Mode now means "Popup open". Express Key 1, Super+W and Esc open or
    close it; the Pen no longer opens it by coming into range, which also
    removes the reopen caused by the Pen Buttons' proximity flicker.
  - Same position, same idea in both modes: 1 open/close, 2 Commit / right
    click, 3 Paste as picture / Ctrl+V, 4 New line / Enter, 5-6 Ink size /
    zoom, 7-8 Undo/Redo. Pen Buttons are Undo/Redo everywhere. Undo/Redo act
    on the Popup's strokes first, then send Ctrl+Z/Ctrl+Y (the redo key in
    Xournal++ and LibreOffice). `undo-stroke` was removed.
  - Esc closes the Popup and keeps its Ink instead of discarding it.
  - The Cheatsheet names what keys do ("Paste", "Zoom") instead of key
    combos, merges paired keys ("5/6 Ink size"), and shows Ink size as a
    percentage (3.0 px/mm = 100%, ±10% per press).
  - Desktop Mode's art-app defaults (E, [, ]) were dropped.

- **Hold to erase** (user request): `pen-buttonN-hold = "eraser"`, default on
  the upper button in both modes. A button with a hold runs its click action
  on release, and only if the Pen did not touch the Tablet while it was held.
  In Pad Mode the hold erases Popup strokes; in Desktop Mode the X eraser
  device stays attached so the app's own eraser works (decopad floats it
  only when the upper button has a click action and no hold).

## Affected Modules
| Module | Role |
|---|---|
| `pen` | Folds Pen evdev events into one state per report |
| `ink` | Strokes in Tablet units: undo, clear, erase, placement on screen |
| `render` | Popup canvas, Cheatsheet text and the Picture Paste image |
| `popup` | The X window and incremental upload of changed pixels |
| `preview` | Caret and Ink preview in the target app |
| `xconn` | Pointer query and warp, key grabs, clipboard ownership |
| `replay` | Virtual pen and its worker thread |
| `daemon` | Mode, Popup lifecycle, Commit, and Express Key dispatch |

## Implementation Plan
1. Pen state tracking; route Pen reports to the Pad Mode logic.
2. Ink model with placement maths and unit tests.
3. Canvas rendering (ruled lines, Ink, Pen Tip ring, Cheatsheet).
4. Popup window, placement near the Anchor, dirty-rectangle uploads.
5. X integration: Popup Toggle and Popup keys, Pointer, clipboard.
6. Virtual pen, Replay thread and completion signal.
7. Commit, pause Commit, new line, Picture Paste, Ink Scale, eraser.
8. Config `[popup]` table and pen buttons; Cheatsheet built from config.

## Testing
`tools/e2e.py` drives decopad with a simulated Tablet (`DECOPAD_TABLET`
selects it; `setup/70-xppen-deco640.rules` grants access to it) and writes into
Xournal++. Final run on 2026-10-17: all 18 checks passed (Popup placement and
persistence, Replay and continuation, new line, Picture Paste, writing-only
key grabs, move, resize, memory, Popup Toggle, covered Replay, undo, eraser,
Ink Scale, config reload with pause Commit, Esc, Desktop/Pad Mode Pointer
behaviour).

Bugs found and fixed while testing:
- Events read from the X socket while waiting for a reply were dropped by an
  extra `events()` call in the main loop; they are now handled before polling.
- The eraser only hit stroke points, missing touches between them; it now
  measures the distance to each segment.
- Picture Paste lost the clipboard race after a Replay (see Decisions).
- The X wacom driver's 4-sample smoothing made replayed points lag up to 75%;
  decopad turns it off for its virtual pen.

## Outcome
Done 2026-10-17. Benchmarks (tools/bench.py) led to two optimizations:
an incremental preview (writing CPU flat at about 1.5% instead of climbing
from 6.5% to 15%) and Ramer–Douglas–Peucker simplification plus a 2 ms
replay interval (a 34-stroke Commit went from 12.9 s to 3.1 s). The release
build is 1.6 MB with LTO.

Surprises: the upper Pen Button reports as an eraser tool; disabling X's
eraser device silences the stylus too; apps lose an immediate Ctrl+V after a
clipboard change; and the X wacom driver smooths uinput tablets by default.
