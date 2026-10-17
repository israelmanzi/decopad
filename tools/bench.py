"""Measures decopad's CPU and memory use with a simulated Tablet.

Usage: python3 -I tools/bench.py DECOPAD_BINARY OUTPUT_DIR [SECONDS]

Stop any running decopad first. Each scenario runs for SECONDS (default 10)
with the Pen reporting at the Deco 640's rate of about 200 reports per second.
CPU is decopad's user+system time over the scenario as a share of one core.
The Pointer moves during the run, so leave the desktop alone.
"""
import importlib.util
import math
import os
import subprocess
import sys
import time
from pathlib import Path

from evdev import ecodes as e

spec = importlib.util.spec_from_file_location("e2e", Path(__file__).with_name("e2e.py"))
e2e = importlib.util.module_from_spec(spec)
spec.loader.exec_module(e2e)

RATE = 200
TICKS = os.sysconf("SC_CLK_TCK")


def cpu_seconds(pid):
    fields = open(f"/proc/{pid}/stat").read().rsplit(")", 1)[1].split()
    return (int(fields[11]) + int(fields[12])) / TICKS


def rss_mb(pid):
    for line in open(f"/proc/{pid}/status"):
        if line.startswith("VmRSS:"):
            return int(line.split()[1]) / 1024
    return 0.0


def pen_report(tablet, x, y, pressure, touching):
    tablet.pen.write(e.EV_ABS, e.ABS_X, int(x))
    tablet.pen.write(e.EV_ABS, e.ABS_Y, int(y))
    tablet.pen.write(e.EV_ABS, e.ABS_PRESSURE, pressure if touching else 0)
    tablet.pen.write(e.EV_KEY, e.BTN_TOOL_PEN, 1)
    tablet.pen.write(e.EV_KEY, e.BTN_TOUCH, 1 if touching else 0)
    tablet.pen.syn()


def drive(tablet, seconds, writing):
    """Moves the Pen at RATE reports per second. Writing draws short
    handwriting-like strokes across the whole Tablet, so Ink keeps growing."""
    start = time.monotonic()
    n = 0
    while (now := time.monotonic()) - start < seconds:
        t = n / RATE
        stroke, phase = divmod(n, 120)  # 0.6 s per stroke
        row, col = divmod(stroke, 12)
        x = 800 + col * 1200 + phase * 8 + 150 * math.sin(phase / 6)
        y = 1200 + (row % 5) * 1500 + 400 * math.sin(phase / 9)
        touching = writing and phase < 100
        pen_report(tablet, x if writing else 8000 + 3000 * math.sin(t), y if writing else 4500 + 2000 * math.cos(t),
                   7000, touching)
        n += 1
        next_at = start + n / RATE
        if next_at > now:
            time.sleep(next_at - now)
    tablet.pen.write(e.EV_KEY, e.BTN_TOUCH, 0)
    tablet.pen.syn()
    return n


def measure(pid, name, action, seconds):
    cpu0, t0 = cpu_seconds(pid), time.monotonic()
    reports = action()
    cpu1, t1 = cpu_seconds(pid), time.monotonic()
    share = (cpu1 - cpu0) / (t1 - t0) * 100
    print(f"{name:<34} {share:6.2f}% CPU  {rss_mb(pid):6.1f} MB RSS  {reports or 0:6d} pen reports")


def main():
    binary, out = Path(sys.argv[1]).resolve(), Path(sys.argv[2]).resolve()
    seconds = float(sys.argv[3]) if len(sys.argv) > 3 else 10.0
    (out / "config" / "decopad").mkdir(parents=True, exist_ok=True)
    (out / "config" / "decopad" / "config.toml").write_text(e2e.run(str(binary), "--default-config") + "\n")
    # Keep a real Tablet's Pen quiet so it cannot move the Pointer mid-run.
    for device in [e2e.InputDevice(path) for path in e2e.list_devices()]:
        if device.name.startswith("XPPen") and device.name.endswith(" Pen"):
            device.grab()
    tablet = e2e.FakeTablet()
    time.sleep(1.0)
    # The Commit replays into whatever is under the Pointer: use Xournal++.
    xournal = subprocess.Popen(["xournalpp"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    window = e2e.run("xdotool", "search", "--sync", "--onlyvisible", "--pid", str(xournal.pid)).splitlines()[0]
    time.sleep(2.0)
    e2e.run("xdotool", "windowactivate", "--sync", window)
    e2e.run("xdotool", "mousemove", "300", "400")
    env = dict(os.environ, DECOPAD_TABLET="decopad-test", XDG_CONFIG_HOME=str(out / "config"),
               XDG_STATE_HOME=str(out / "state"))
    decopad = subprocess.Popen([str(binary)], env=env, stderr=open(out / "decopad.log", "w"))
    try:
        time.sleep(3.0)
        pid = decopad.pid
        print(f"binary {binary.stat().st_size / 1e6:.1f} MB, startup RSS {rss_mb(pid):.1f} MB")
        measure(pid, "idle, pad closed", lambda: time.sleep(seconds), seconds)
        measure(pid, "hovering, pad closed (Desktop)", lambda: drive(tablet, seconds, False), seconds)
        tablet.away()
        tablet.express_key([e.KEY_B])
        time.sleep(0.5)
        measure(pid, "idle, pad open", lambda: time.sleep(seconds), seconds)
        measure(pid, "hovering, pad open", lambda: drive(tablet, seconds, False), seconds)
        measure(pid, "writing, pad open (Ink grows)", lambda: drive(tablet, seconds, True), seconds)
        measure(pid, "writing more (bigger Ink)", lambda: drive(tablet, seconds, True), seconds)
        tablet.away()
        log = out / "decopad.log"
        tablet.express_key([e.KEY_E])
        for _ in range(600):
            time.sleep(0.1)
            replays = [l for l in log.read_text().splitlines() if "replayed" in l]
            if replays:
                print(f"{'commit of all that Ink':<34} {replays[-1].split(': ', 1)[1]}")
                break
        tablet.express_key([e.KEY_B])
    finally:
        decopad.terminate()
        decopad.wait()
        xournal.kill()
        tablet.keyboard.close()
        tablet.pen.close()


if __name__ == "__main__":
    main()
