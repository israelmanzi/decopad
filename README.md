# decopad

decopad turns an **XP-Pen Deco 640** drawing tablet into a companion for
your keyboard on Linux (X11). It has two modes:

- **Desktop Mode** (writing pad closed): the pen is a normal tablet pen, and
  the tablet's keys and pen buttons are shortcuts.
- **Pad Mode** (writing pad open): a small writing pad appears. The pen
  writes only into the pad while the pointer stays where your text should
  go. A blue preview shows the ink there as you write, and **Commit**
  redraws it into the app at that spot (Xournal++, LibreOffice Draw, Krita,
  …).

It is a single 1.6 MB Rust binary that idles at 0% CPU and uses about 1.5%
of one core while you write.

## Requirements

- Fedora (or similar) on **X11**, tested with XFCE. Wayland is not supported.
- The X wacom driver, which Fedora installs by default
  (`xorg-x11-drv-wacom`). It provides `xsetwacom`.
- `xinput`, `notify-send` (libnotify) and `fc-match` (fontconfig).
- A Rust toolchain (`cargo`), version 1.88 or newer.

```bash
sudo dnf install xinput xorg-x11-drv-wacom libnotify fontconfig
```

## Install

```bash
git clone git@github.com:israelmanzi/decopad.git
cd decopad
./setup/install.sh
```

The script:

1. Installs a udev rule so your login can use the tablet and `/dev/uinput`
   (`setup/70-xppen-deco640.rules`). This needs sudo.
2. Loads the `uinput` module now and at every boot (`setup/uinput.conf`).
3. Builds and installs `~/.cargo/bin/decopad`.
4. Adds an autostart entry so decopad starts when you log in, and starts it
   now.

The log goes to `~/.cache/decopad.log`. Run the script again to update.

## Using it

### Tablet keys

The eight Express Keys are numbered by the shortcut each one typed before
decopad (key 1 = `B`, key 2 = `E`, …, key 8 = `Ctrl+Shift+Z`). The same
position means the same idea in both modes:

| Key | Pad open (writing) | Pad closed (pen = mouse) |
|---|---|---|
| 1 | Close the pad | Open the pad |
| 2 | **Commit**: write the ink at the pointer | Right click |
| 3 | Paste the ink as a picture | Paste (`Ctrl+V`) |
| 4 | New line: commit, then continue one line down | `Enter` |
| 5 / 6 | Smaller / larger ink | Zoom out / in |
| 7 / 8 | Undo / redo | Undo / redo |

### Pen buttons

| Button | Click | Hold while touching the tablet |
|---|---|---|
| Lower | Undo | — |
| Upper | Redo | **Erase** (the pad's strokes, or the app's own eraser) |

Undo and redo work on the pad's strokes first. When the pad has nothing to
undo or redo, they send `Ctrl+Z` / `Ctrl+Y` to the app, so the lower button
can take back the last commit.

### Keyboard

| Key | Does |
|---|---|
| `Super+W` | Open or close the pad (same as key 1) |
| `Enter` | New line |
| `Backspace` | Clear the pad |
| `Tab` | Eraser on/off |
| `Esc` | Close the pad (its ink is kept) |

`Enter`, `Backspace`, `Tab` and `Esc` belong to the pad only **while you are
writing**: when the pen is near the tablet, or the pad holds ink you have not
committed. Otherwise they work as usual in your apps, even with the pad
open.

### Writing pad

- **Drag** the pad with the mouse to move it, or drag its **bottom-right
  corner** to resize it. decopad remembers both.
- Write on the ruled lines. The line where you start writing lands on the
  pointer, and lines below it keep their layout, so you can write several
  lines and commit once.
- After a commit, the pointer moves to just after the delivered ink, so the
  next commit continues the line. Move the pointer yourself to write
  somewhere else.
- The legend at the bottom of the pad lists every control.

**App tips**

- In **LibreOffice Draw or Impress**, double-click the *Freeform Line* tool
  so it stays selected; otherwise only the first replayed stroke is drawn.
- **Paste as picture** lands wherever the app puts pastes. In Xournal++ that
  is the middle of the view, not the pointer. It also replaces what was on
  your clipboard.

## Configuration

`~/.config/decopad/config.toml` is created on first run and explains every
option. Changes apply as soon as you save; if a change has a mistake,
decopad keeps the previous settings and shows a notification saying what
is wrong. `decopad --default-config` prints the default file.

Things you might change:

- Any key or pen button: a shortcut such as `"ctrl+shift+s"`,
  `"right-click"`, `"toggle-pad"`, `"undo"`, `"commit"`, … (the file lists
  them all).
- `pen-button2-hold = "eraser"`: hold-to-erase. Set it on `pen-button1-hold`
  for the lower button; in Desktop Mode only the upper button can use the
  app's eraser, because that is the one the pen reports as an eraser.
- `[popup]`: the pad's starting width, ink size, line spacing, an automatic
  commit after a pause (`commit-delay-ms`, off by default) and the replay
  speed (`replay-interval-ms`).

## Performance

Measured with `tools/bench.py` (simulated tablet at 200 reports/s, release
build, 2026-10-17):

| Scenario | CPU (one core) | Memory |
|---|---|---|
| Idle, pad closed | 0.0% | 14 MB |
| Pen hovering, pad closed | 0.4% | 14 MB |
| Idle, pad open | 0.2% | 20 MB |
| Writing, pad open (flat as ink grows) | 1.3–1.5% | 21 MB |
| Commit of 34 long strokes | 3.1 s | — |

Optimisations behind these numbers:

- The preview's ink is drawn incrementally and only the new pixels are added
  to its window shape. Before, CPU climbed from 6.5% to 15% as ink grew.
- Replayed strokes are simplified (Ramer–Douglas–Peucker, 0.2 px and 8%
  pressure tolerance), cutting points by about 60% with no visible
  difference.
- Points are replayed 2 ms apart; Xournal++ reproduced strokes exactly even
  at 1 ms.
- The release build uses LTO and is stripped.

## How it works

- decopad finds the tablet by USB ID `28bd:2904` and takes exclusive control
  (`EVIOCGRAB`) of its keyboard node. The keys arrive as fixed factory
  shortcuts, which decopad decodes back into "Express Key N".
- In Pad Mode it also grabs the pen, so X stops moving the pointer, and
  draws the pen's input into the pad. In Desktop Mode it releases the pen to
  X's wacom driver.
- **Commit** replays the strokes through a virtual pen (uinput) at the
  pointer. The X driver's smoothing is switched off for that pen so strokes
  land pixel-exactly.
- Shortcuts go through a virtual keyboard, so they behave like a real one.
- If decopad stops, the tablet keys fall back to their factory shortcuts.
  The pen's side buttons stay without their X actions until you replug the
  tablet.

More detail: `docs/CONTEXT.md` (glossary), `docs/hardware-deco-640.md`
(captured reports), `docs/adr/` (decisions) and `docs/worklog/` (how each
feature was built and tested).

## Development

```bash
cd decopad
cargo test                 # unit tests
cargo clippy --all-targets
```

The tools need `python3-evdev`, `xdotool`, ImageMagick and Xournal++. Stop
the installed decopad first (`pkill -x decopad`); both tools take over the
pointer for a minute.

- `python3 -I tools/e2e.py decopad/target/debug/decopad /tmp/e2e`: the
  end-to-end suite. It drives decopad with a simulated tablet into Xournal++
  and saves screenshots of each step.
- `python3 -I tools/bench.py decopad/target/release/decopad /tmp/bench`:
  the CPU and memory benchmark.
- `python3 -I tools/capture.py 60 /tmp/capture.log`: records everything the
  real tablet sends.

`DECOPAD_TABLET=<name prefix>` makes decopad attach to a different device
name, which is how the tools use a simulated tablet while the real one stays
plugged in.

## Status

Tasks 1–3 in `docs/TASKS.md` are done. Handwriting-to-text at the text
cursor (task 4) is planned.
