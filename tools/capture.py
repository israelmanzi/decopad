"""Record everything the Deco 640 sends: evdev events and raw HID reports.

Usage: python3 tools/capture.py SECONDS OUTFILE
Pen motion is summarised (only button/proximity changes are logged).
"""
import os
import select
import sys
import time

import evdev
from evdev import ecodes

VENDOR, PRODUCT = 0x28BD, 0x2904


def tablet_hidraw_nodes():
    nodes = []
    for name in sorted(os.listdir("/sys/class/hidraw")):
        uevent = open(f"/sys/class/hidraw/{name}/device/uevent").read()
        if f"HID_ID=0003:{VENDOR:08X}:{PRODUCT:08X}" in uevent:
            nodes.append(f"/dev/{name}")
    return nodes


def main():
    seconds, outfile = float(sys.argv[1]), sys.argv[2]
    devices = [
        evdev.InputDevice(path)
        for path in evdev.list_devices()
        if (d := evdev.InputDevice(path)).info.vendor == VENDOR and d.info.product == PRODUCT
    ]
    hidraw = {os.open(p, os.O_RDONLY | os.O_NONBLOCK): p for p in tablet_hidraw_nodes()}
    by_fd = {d.fd: d for d in devices}
    start = time.monotonic()
    last_raw = {}
    with open(outfile, "w", buffering=1) as out:
        for d in devices:
            out.write(f"# evdev {d.path} {d.name}\n")
        for p in hidraw.values():
            out.write(f"# hidraw {p}\n")
        while (left := seconds - (time.monotonic() - start)) > 0:
            ready, _, _ = select.select(list(by_fd) + list(hidraw), [], [], left)
            for fd in ready:
                t = time.monotonic() - start
                if fd in hidraw:
                    data = os.read(fd, 64)
                    # Pen reports stream constantly; log them only when non-position bytes change.
                    if data[0] == 0x07 and last_raw.get(fd) == data[1]:
                        continue
                    last_raw[fd] = data[1]
                    out.write(f"{t:8.3f} RAW {hidraw[fd]} {data.hex(' ')}\n")
                    continue
                for e in by_fd[fd].read():
                    if e.type in (ecodes.EV_SYN, ecodes.EV_MSC):
                        continue
                    if e.type == ecodes.EV_ABS and by_fd[fd].name.endswith("Pen"):
                        continue
                    name = ecodes.bytype[e.type].get(e.code, e.code)
                    out.write(f"{t:8.3f} EV  {by_fd[fd].name}: {ecodes.EV[e.type]} {name} {e.value}\n")


if __name__ == "__main__":
    main()
