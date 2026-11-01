# Android layer: handoff (2026-11-01)

Goal: make the XP-Pen Deco 640 work as a system-wide "S Pen" on the Nothing
phone (Android 16, model A015 "Tetris", MT6878, arm64, 1080x2400, no root).

## Proven on the device

- Tablet over OTG is seen by Android as a generic HID **stylus**
  (`Sources: KEYBOARD|MOUSE|STYLUS`, pressure/tilt/orientation/hover).
  Its second interface (`… Mouse`) is classified **TOUCH_SCREEN** (ghost-finger
  risk); the third (keyboard) types the Express Keys' factory shortcuts.
- InputReader maps the tablet to 4266x2400 (wider than the screen) — area
  mapping is wrong in portrait; this is the known "one orientation" problem.
- `/dev/uhid` is openable by the **shell UID** (group 3011). A shell-side
  binary created a virtual pen; Android registered it as a real stylus with
  pressure and tilt. No root.
- A normal app (spike in `android/`) can `claimInterface(force=true)` all three
  interfaces: kernel drivers are detached, the app receives raw pen reports
  (797 captured, `07 a0 …` ~200 Hz). Replug the tablet to give it back to the
  kernel.
- Shizuku 13.6.0 installed and running (`shizuku_server`, uid 2000, groups
  include 3011 uhid). Start command (repeat after every reboot):
  `adb shell /data/app/~~ukSyWVP-w2-T8tvnQMndFA==/moe.shizuku.privileged.api-u0F6GQ2GiurC9BE1q8na2Q==/lib/arm64/libshizuku.so`
- adb over Tailscale: `adb connect 100.78.144.65:<port>` (port changes when
  wireless debugging is toggled). BIT-Web Wi-Fi isolates clients; mDNS is
  blocked, so Tailscale is the working transport.

## Architecture (chosen)

App (USB owner) + Shizuku user service (uhid owner):

1. App claims the tablet's HID interfaces (force) and reads reports.
2. App forwards pen reports over binder to a Shizuku UserService.
3. The service writes them to `/dev/uhid` as a virtual stylus (descriptor from
   `decopad/src` capture: report 7, 16-bit X/Y, 14-bit pressure, tilt).
4. Express keys are decoded in the app (unique combos, see
   `docs/hardware-deco-640.md`) and mapped to configurable actions; shortcuts
   are injected through a uhid virtual keyboard.
5. Pen area mapping → screen region, pressure curve, per-app profiles.

Verified facts to reuse: pen ranges 15999x8999, pressure 0..16383, tilt ±127;
Express Key combos B, E, [, ], Ctrl+KP-, Ctrl+KP+, Ctrl+Z, Ctrl+Shift+Z; upper
pen button reports as eraser tool, lower as BTN_STYLUS.

## Implemented (2026-11-01)

Dependencies: `dev.rikka.shizuku:api:13.1.5` + `provider:13.1.5`; AIDL is
enabled; `ShizukuProvider` is declared with `${applicationId}.shizuku`.

Pieces (all in `android/`):

- `org.decopad.pen.IPenService` — `create(byte[] descriptor)`,
  `oneway submit(byte[] report)`, `isOpen()`, and the reserved `destroy()`.
- `org.decopad.pen.PenUserService` — the requested Shizuku UserService. It
  loads `libdecopad_uhid.so` (JNI, `uhid_jni.c`) and owns `/dev/uhid`.
- `BridgeService` — foreground USB owner. Reads the pen interface's **real**
  HID report descriptor with `GET_DESCRIPTOR` (109 bytes), creates the virtual
  device from it, force-claims all three interfaces, and forwards every raw
  report 7 byte-for-byte. No report parsing is needed because the virtual
  descriptor is the tablet's own.
- `PenDescriptors` — the same 109-byte descriptor as a fallback, plus hover /
  out-of-range reports for the self test.
- `ShellPen` + `uhid_helper.c` — fallback transport (see below).

**MediaTek blocker.** Shizuku user services cannot start on this phone. Every
attempt dies in `LoadedApk.makeApplicationInner` with an
`Object.equals` NPE before our class is constructed — the OEM patches
`makeApplication` to call `Application.getProcessName()`, which is null in
Shizuku's `app_process` helper. Known upstream:
[Shizuku #1198](https://github.com/RikkaApps/Shizuku/issues/1198) and
[#2048](https://github.com/RikkaApps/Shizuku/issues/2048). Nothing in the app
can patch that call site.

Fallback: after 4 s without a user-service connection, `BridgeService` starts
`libdecopad_uhid_helper.so` through `IShizukuService.newProcess(...)` (the
`moe.shizuku.server` AIDL comes with the Shizuku `api` artifact). That process
runs as uid shell with the uhid group, so it can open `/dev/uhid`. The app
talks to it over stdin/stdout with length-prefixed frames
(`create` / `input` / `destroy`). The helper is packaged as
`lib/arm64-v8a/libdecopad_uhid_helper.so` only so AGP puts it in a directory
shell is allowed to execute. The UserService path stays first for ROMs where
it works.

Verified on the phone (`Nothing A015`, Android 16):

- Shizuku permission granted through the app.
- Helper transport starts; `/dev/uhid` create succeeds.
- The virtual device registers as
  `XPPen Deco 640 Pen (decopad) Pen`, `Sources: KEYBOARD | MOUSE | STYLUS`,
  with the same axes as the tablet (X 0..15999, Y 0..8999, pressure 0..16383,
  tilt ±127).
- Self test (hover-only, 150 reports) produces `BTN_TOOL_PEN` down/up and
  `ABS_X`/`ABS_Y` on the virtual evdev node; stopping the bridge removes the
  node and kills the helper.
- Interfaces 0, 1, 2 are force-claimed; interface 1 is the pen (report 7).
- **Real pen, real forwarding:** ~200 reports/s reach the helper. A tip-down
  report `07 a1 fc 21 ff 0f 29 02 18 f6` (X 8700, Y 4095, pressure 553,
  tilt 24/−10) came through, and the virtual evdev node reported
  `ABS_PRESSURE 623` from a later press, with `BTN_TOOL_PEN` in range and
  `ABS_X`/`ABS_Y`/`ABS_TILT_X`/`ABS_TILT_Y` following the pen.

### Area mapping (implemented)

`AreaMapper` rewrites report 7's X/Y before the helper submits it. InputReader
fits an external stylus so the device axis along the display's long side fills
that side; measured from `dumpsys input` for all four rotations:

```
portrait  (rotation 0/2): pointer = raw * (2400 / 8999)   = raw * 0.2667
landscape (rotation 1/3): pointer = raw * (2400 / 15999)  = raw * 0.1500
```

The pointer mapping is direct in all four rotations. The mapper maps the
tablet's active area into a region of the current display (normalized
`left/top/right/bottom`, full screen by default), then divides by that scale
to get the raw values to send. Two modes:

- **stretch** (default): the whole active area fills the region. Reaches the
  whole screen; X/Y sensitivity differ when the aspect ratios differ.
- **fit**: aspect-preserving, centered in the region (portrait full screen
  gives a 1080x607 band).

Tablet axes follow the phone by default (`auto`): a portrait phone expects
the tablet turned 90 degrees clockwise (its original right edge down, left
edge up) with the long side vertical; a landscape phone expects the natural
landscape tablet. The setting cycles auto -> landscape ->
portrait CCW -> portrait CW (the two rotations of the physical tablet), so a
mirrored turn can be corrected. Use the tablet's long side vertical in
portrait: the axis ratio is then 0.12 vs 0.15 px/unit (1.25:1) instead of
3.95:1, which is what made portrait text narrow and horizontal lines curved.
Aspect-preserving `fit` remains available for exact 1:1.

Both settings are two buttons in the app and persist in the `mapping`
SharedPreferences (`mode`, `tablet`, `left`, `top`, `right`, `bottom`).

The region itself has a picker (`RegionPickerActivity`): a scaled phone
outline with the current region; drag a corner to resize, drag inside to
move, or drag in empty space to draw a new rectangle. Presets are
**Full screen** and **Tablet 16:9** (the largest centered region with the
tablet's aspect ratio, so stretch mode is undistorted). Save persists the
normalized region; the mapper uses it live. Verified on-device: the 16:9
preset gave L0% T37% R100% B63% in portrait, a hand-drawn rectangle persisted,
and the main screen status shows the region.

Measured checks: with landscape axes, the self test's tablet X 4000..12940
becomes virtual ABS_X 1012..3274. With auto axes in portrait (clockwise), the
same test becomes an increasing virtual ABS_Y sweep (2992..5591 in the
captured window) at constant ABS_X, i.e. the tablet was rotated as intended.

Also fixed: HID `GET_DESCRIPTOR` now requests the exact `wDescriptorLength`
from the configuration descriptor. Oversized 4096-byte requests stalled on
this tablet after the first session; the code also retries once with a fresh
connection and falls back to interface 1 (the pen) if descriptors stay
unreadable.

### Pen buttons: Backspace and Undo

On the press edge, the top (smaller) button sends Ctrl+Z (undo) and the
bottom (larger) button sends Backspace. Per user testing the top/smaller
button is status bit 1 and the bottom/larger one is bit 2. Both bits are masked from the forwarded
report, so apps no longer see `BTN_STYLUS` or the eraser tool. A 300 ms
debounce absorbs the upper button's out-of-range blip.

The shell helper injects through a **short-lived uhid keyboard**: it is
created on the first action, waits for `UHID_OPEN`, sends the key chord
(backspace = usage 0x2A; undo = Left Ctrl + Z), and is destroyed after 1.5 s
idle. That makes both actions instant and allows chords, while Android never
sees a permanent hardware keyboard (which would hide the on-screen IME).
The user-service transport only supports Backspace (`input keyevent 67`); this
Android build has no `input keycombination`, and the ROM cannot run user
services anyway.

### Tests and structure

Pure logic is split out and unit-tested without a device:

- `Mapping` — area mapping math (stretch/fit, tablet rotations, auto).
- `PenProtocol` — report 7 parsing, button mask, coordinate setters.
- `ButtonActions` — press-edge detection and debounce.

```
gradle -p android :app:testDebugUnitTest   # 13 tests, all passing
```

Not yet verified: drawing in **ibisPaint X** / **Squid** (neither is installed
on the phone; Play Store is).

Build and run:

```
~/.gradle/wrapper/dists/gradle-8.14.3-bin/*/gradle-8.14.3/bin/gradle \
    -p android :app:assembleDebug
adb install -r android/app/build/outputs/apk/debug/app-debug.apk
adb shell am start -n org.decopad.spike/.MainActivity
adb logcat -s decopad-bridge decopad-helper decopad-uhid
```

The app auto-starts the bridge when the tablet is attached and Shizuku
permission is granted. "Test virtual pen (hover only)" creates the fallback
descriptor and moves hover X without touching, so it is safe to run anywhere.

## Next steps

1. Validate with the pen physically on the tablet, then in **ibisPaint X**
   and **Squid** (install from Play Store).
2. Express-key mapping; overlay palette; per-app profiles (region +
   pressure curve per app).
3. Tests: `python3 -I tools/e2e.py` style harness cannot run against the phone;
   use adb screenshots + logcat.

## App research summary

- Notes: **Squid** (pressure, palm rejection, eraser, button undo/redo);
  **Saber** (FOSS, pressure curves). OneNote/Xodo/Nebo partial.
- Drawing: **ibisPaint X** (official XP-Pen USB list), **Sketchbook**.
- Handwriting input (Bihua web app, Pleco, Google Translate, Gboard):
  stylus-as-touch works; palm rejection is what matters.
- Chrome: stylus reaches web content as `pointerType:"pen"` with pressure;
  text-field handwriting works on Android 13+ with a supporting IME (Gboard).

## Decisions

- The Android app lives in `android/` of this repository (spike project already
  there).
- Area mapping supports **both** portrait and landscape: map the tablet's
  active area to the chosen screen region for either orientation, with a
  per-app or manual orientation setting.

## Open questions

- None blocking; start implementation.
