# 0002 Desktop Mode hands the Pen back to the X wacom driver

**Date:** 2026-10-17
**Status:** Accepted

## Context
The Pen must move the Pointer in Desktop Mode and must not in Pad Mode. On
this machine the X server already drives the Pen through `xorg-x11-drv-wacom`,
with pressure, tilt, the eraser and area mapping working natively. An
exclusive evdev grab (`EVIOCGRAB`) on the Pen node succeeds while X has it
open, and stops X from receiving its events until the grab is released.

## Decision
decopad grabs the Pen node only while in Pad Mode and releases it in Desktop
Mode, so in Desktop Mode the native X driver moves the Pointer exactly as it
did before decopad existed. decopad sets the driver's area so the Active Area
keeps the screen's aspect ratio.

## Alternatives considered
- **Always grab and re-emit through a decopad virtual tablet:** one owner for
  all Pen input and full control over mapping, but adds a hop to every Pen
  report and reimplements pressure, tilt and mapping the driver already does.

## Consequences
- Desktop Mode latency and behaviour are identical to the plain driver, and
  stopping decopad leaves the Pen fully working.
- Mapping options are limited to what the wacom driver exposes.
- Mode switches while the Pen tip is down need care so X never sees a stroke
  without its release.
