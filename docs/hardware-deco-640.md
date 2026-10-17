# XP-Pen Deco 640 hardware notes

Observed on Fedora 43, kernel 7.2.11, XFCE on X11, 2026-10-17.

- USB ID `28bd:2904`, bound to `hid-generic`. `hid-uclogic` has no alias for
  this product ID, and libwacom does not know it.
- The X server drives the pen through `xorg-x11-drv-wacom` (its
  `MatchIsTablet` class claims any evdev tablet), exposed as
  `XPPen Deco 640 Pen stylus` and `… Pen eraser`, so `xsetwacom` and the
  `Wacom Tablet Area` property work on it. The mouse and keyboard nodes go
  through `xorg-x11-drv-libinput`.
- Screen: one 2560×1440 monitor (DP-2), the same 16:9 shape as the Active Area.

## HID interfaces

| Interface | evdev name | Reports |
|---|---|---|
| `…:2904.0008` | `XPPen Deco 640 Mouse` / `XPPen Deco 640 Keyboard` | 1: relative mouse, 9: absolute mouse with pressure, 6: boot keyboard (Factory Mode Express Keys) |
| `…:2904.0009` | `XPPen Deco 640 Pen` | 7: digitizer pen |
| `…:2904.000A` | (hidraw only) | 2: vendor page `0xFF0A`, 11-byte input, 19-byte output |

## Pen report (ID 7)

- Bits: tip, barrel 1, eraser, invert, in-range.
- X: 0–15999 over 6.299 in. Y: 0–8999 over 3.543 in (about 2540 lines/in).
- Pressure: 0–16383. Tilt X/Y: −127…127.

Status byte (second byte of report 7): `0xa0` in range, `0xa1` tip down,
`0xc0` out of range.

## Express Keys (Factory Mode)

Captured 2026-10-17 with `tools/capture.py`. Eight keys, in the order pressed;
each arrives as report 6 on the keyboard interface, as one press then release:

| # | Combo | Report 6 bytes |
|---|---|---|
| 1 | `B` | `06 00 05` |
| 2 | `E` | `06 00 08` |
| 3 | `[` | `06 00 2f` |
| 4 | `]` | `06 00 30` |
| 5 | `Ctrl`+`KP-` | `06 01 56` |
| 6 | `Ctrl`+`KP+` | `06 01 57` |
| 7 | `Ctrl`+`Z` | `06 01 1d` |
| 8 | `Ctrl`+`Shift`+`Z` | `06 03 1d` |

Every combo is distinct, so each key can be identified without Raw Mode.

## Open questions

- Pen Buttons: no button bits or `BTN_STYLUS` events appeared in two captures.
- The Raw Mode command sent on the vendor interface (the vendor driver or a
  capture will tell).
