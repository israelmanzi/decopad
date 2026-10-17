"""End-to-end check of decopad with a simulated Tablet, writing into Xournal++.

Usage: python3 -I tools/e2e.py DECOPAD_BINARY OUTPUT_DIR

Stop any running decopad first. The script creates fake Tablet devices,
opens Xournal++, runs decopad against the fakes with a throwaway config, and
saves screenshots of each step into OUTPUT_DIR. It moves the Pointer and
types into the Xournal++ window it opened, so leave the desktop alone while
it runs (about a minute).
"""
import math
import os
import subprocess
import sys
import time
from pathlib import Path

from evdev import AbsInfo, InputDevice, UInput, ecodes as e, list_devices

VENDOR, PRODUCT = 0x28BD, 0x2904


def run(*cmd):
    return subprocess.run(cmd, capture_output=True, text=True).stdout.strip()


def pointer():
    loc = dict(part.split(":") for part in run("xdotool", "getmouselocation").split()[:2])
    return int(loc["x"]), int(loc["y"])


def shot(out, name, crop=None):
    args = ["import", "-window", "root"]
    if crop:
        args += ["-crop", "%dx%d+%d+%d" % crop]
    subprocess.run(args + [str(out / f"{name}.png")], check=True)


def popup_geometry():
    """(x, y, width, height) of the visible Popup, or None."""
    ids = run("xdotool", "search", "--onlyvisible", "--name", "^decopad$").split()
    if not ids:
        return None
    info = dict(l.strip().split(": ", 1) for l in run("xdotool", "getwindowgeometry", ids[0]).splitlines()[1:])
    x, y = map(int, info["Position"].split()[0].split(","))
    w, h = map(int, info["Geometry"].split("x"))
    return x, y, w, h


def drag(start, end, steps=15):
    run("xdotool", "mousemove", *map(str, start))
    time.sleep(0.1)
    run("xdotool", "mousedown", "1")
    for i in range(1, steps + 1):
        run("xdotool", "mousemove", str(start[0] + (end[0] - start[0]) * i // steps),
            str(start[1] + (end[1] - start[1]) * i // steps))
        time.sleep(0.02)
    run("xdotool", "mouseup", "1")
    time.sleep(0.3)


class FakeTablet:
    def __init__(self):
        keys = [e.KEY_LEFTCTRL, e.KEY_LEFTSHIFT, e.KEY_B, e.KEY_E, e.KEY_LEFTBRACE, e.KEY_RIGHTBRACE,
                e.KEY_KPMINUS, e.KEY_KPPLUS, e.KEY_Z]
        self.keyboard = UInput({e.EV_KEY: keys}, name="decopad-test Keyboard", vendor=VENDOR, product=PRODUCT)
        self.pen = UInput(
            {
                e.EV_KEY: [e.BTN_TOOL_PEN, e.BTN_TOOL_RUBBER, e.BTN_TOUCH, e.BTN_STYLUS],
                e.EV_ABS: [
                    (e.ABS_X, AbsInfo(0, 0, 15999, 0, 0, 100)),
                    (e.ABS_Y, AbsInfo(0, 0, 8999, 0, 0, 100)),
                    (e.ABS_PRESSURE, AbsInfo(0, 0, 16383, 0, 0, 0)),
                ],
            },
            name="decopad-test Pen",
            vendor=VENDOR,
            product=PRODUCT,
        )

    def express_key(self, combo):
        """Types a Factory Mode combo, e.g. [KEY_E] for Express Key 2."""
        for code in combo:
            self.keyboard.write(e.EV_KEY, code, 1)
        self.keyboard.syn()
        time.sleep(0.05)
        for code in reversed(combo):
            self.keyboard.write(e.EV_KEY, code, 0)
        self.keyboard.syn()
        time.sleep(0.1)

    def hover(self, x, y):
        self.pen.write(e.EV_ABS, e.ABS_X, int(x))
        self.pen.write(e.EV_ABS, e.ABS_Y, int(y))
        self.pen.write(e.EV_KEY, e.BTN_TOOL_PEN, 1)
        self.pen.syn()
        time.sleep(0.15)

    def away(self):
        self.pen.write(e.EV_KEY, e.BTN_TOOL_PEN, 0)
        self.pen.syn()
        time.sleep(0.15)

    def button(self, code):
        """Clicks a Pen side button while hovering."""
        self.pen.write(e.EV_KEY, code, 1)
        self.pen.syn()
        time.sleep(0.08)
        self.pen.write(e.EV_KEY, code, 0)
        self.pen.syn()
        time.sleep(0.15)

    def upper_down(self):
        """Presses the upper side button the way the Deco 640's pen reports it:
        the pen tool leaves and the eraser tool is in range while it is held."""
        for code, value in [(e.BTN_TOOL_PEN, 0), (e.BTN_TOOL_RUBBER, 1)]:
            self.pen.write(e.EV_KEY, code, value)
            self.pen.syn()
            time.sleep(0.05)

    def upper_up(self):
        for code, value in [(e.BTN_TOOL_RUBBER, 0), (e.BTN_TOOL_PEN, 1)]:
            self.pen.write(e.EV_KEY, code, value)
            self.pen.syn()
            time.sleep(0.05)
        time.sleep(0.1)

    def upper_button(self):
        self.upper_down()
        self.upper_up()

    def stroke(self, points):
        x, y = points[0]
        self.hover(x, y)
        for i, (x, y) in enumerate(points):
            self.pen.write(e.EV_ABS, e.ABS_X, int(x))
            self.pen.write(e.EV_ABS, e.ABS_Y, int(y))
            self.pen.write(e.EV_ABS, e.ABS_PRESSURE, 6000 + int(4000 * math.sin(i / 6)))
            if i == 0:
                self.pen.write(e.EV_KEY, e.BTN_TOUCH, 1)
            self.pen.syn()
            time.sleep(0.004)
        self.pen.write(e.EV_ABS, e.ABS_PRESSURE, 0)
        self.pen.write(e.EV_KEY, e.BTN_TOUCH, 0)
        self.pen.syn()
        time.sleep(0.05)


def line(a, b, steps=40):
    return [(a[0] + (b[0] - a[0]) * t / steps, a[1] + (b[1] - a[1]) * t / steps) for t in range(steps + 1)]


def circle(cx, cy, r, steps=60):
    return [(cx + r * math.cos(2 * math.pi * t / steps), cy + r * math.sin(2 * math.pi * t / steps)) for t in range(steps + 1)]


def write_hi(tablet, x0, baseline):
    """'Hi' on the ruled line whose baseline is at `baseline`."""
    tablet.stroke(line((x0, baseline - 900), (x0, baseline)))
    tablet.stroke(line((x0 + 500, baseline - 900), (x0 + 500, baseline)))
    tablet.stroke(line((x0, baseline - 450), (x0 + 500, baseline - 450)))
    tablet.stroke(line((x0 + 800, baseline - 550), (x0 + 800, baseline)))
    tablet.stroke(circle(x0 + 800, baseline - 800, 40, 12))


def main():
    binary, out = Path(sys.argv[1]).resolve(), Path(sys.argv[2]).resolve()
    out.mkdir(parents=True, exist_ok=True)
    (out / "config" / "decopad").mkdir(parents=True, exist_ok=True)
    config_text = run(str(binary), "--default-config") + "\n"
    (out / "config" / "decopad" / "config.toml").write_text(config_text)
    log = open(out / "decopad.log", "w")
    # Keep a real Tablet's Pen quiet so it cannot move the Pointer mid-test.
    real_pens = [InputDevice(path) for path in list_devices()]
    real_pens = [d for d in real_pens if (d.info.vendor, d.info.product) == (VENDOR, PRODUCT)
                 and d.name.startswith("XPPen") and d.name.endswith(" Pen")]
    for pen in real_pens:
        pen.grab()
    tablet = FakeTablet()
    time.sleep(1.0)

    xournal = subprocess.Popen(["xournalpp"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    window = run("xdotool", "search", "--sync", "--onlyvisible", "--pid", str(xournal.pid)).splitlines()[0]
    time.sleep(2.0)
    run("xdotool", "windowactivate", "--sync", window)
    geometry = dict(line.strip().split(": ", 1) for line in run("xdotool", "getwindowgeometry", window).splitlines()[1:])
    wx, wy = map(int, geometry["Position"].split()[0].split(","))
    ww, wh = map(int, geometry["Geometry"].split("x"))
    anchor = (wx + 250, wy + 300)
    run("xdotool", "mousemove", *map(str, anchor))
    region = (min(ww, 1500), min(wh, 900), wx, wy)

    env = dict(os.environ, DECOPAD_TABLET="decopad-test", XDG_CONFIG_HOME=str(out / "config"),
               XDG_STATE_HOME=str(out / "state"))
    decopad = subprocess.Popen([str(binary)], env=env, stderr=log)
    try:
        for _ in range(50):
            time.sleep(0.1)
            if "replay pen smoothing" in (out / "decopad.log").read_text():
                break
        report = {"anchor": anchor}

        # 1. Express Key 1 (factory "B") opens the Popup near the Pointer.
        tablet.express_key([e.KEY_B])
        tablet.hover(4000, 3000)
        report["popup"] = popup_geometry()
        shot(out, "1-popup-open")
        # 2. Write "Hi" on the second ruled line (baseline 3000) and a circle.
        write_hi(tablet, 1500, 3000)
        tablet.stroke(circle(3200, 2700, 280))
        tablet.away()
        time.sleep(0.3)
        shot(out, "2-ink-and-preview")
        tablet.hover(4000, 3000)
        # 3. Express Key 2 (factory "E") commits: Replay at the Pointer.
        tablet.express_key([e.KEY_E])
        time.sleep(2.5)
        report["after commit"] = pointer()
        report["popup after commit"] = popup_geometry()
        shot(out, "3-replayed")

        # 4. Continue the line, then Enter: commit and go to the next line.
        write_hi(tablet, 1500, 3000)
        run("xdotool", "key", "Return")
        time.sleep(2.5)
        report["after new line"] = pointer()
        shot(out, "4-continued")

        # 4b. Two lines written before one Commit keep their layout.
        write_hi(tablet, 1500, 3000)
        write_hi(tablet, 1500, 4500)
        tablet.express_key([e.KEY_E])
        time.sleep(3.0)
        run("xdotool", "key", "super+w")
        time.sleep(0.3)
        shot(out, "4b-two-lines")
        run("xdotool", "key", "super+w")
        time.sleep(0.3)
        tablet.hover(4000, 3000)

        # 5. Picture Paste with Express Key 3 (factory "["). Apps choose
        # where pastes land (Xournal++: centre of the view), so hide the Popup
        # for the screenshot.
        tablet.stroke(circle(3000, 2400, 500))
        tablet.express_key([e.KEY_LEFTBRACE])
        time.sleep(1.5)
        run("xdotool", "key", "super+w")
        time.sleep(0.3)
        shot(out, "5-picture-paste")
        run("xdotool", "key", "super+w")
        time.sleep(0.3)

        # 6. Pen away and no Ink: Tab belongs to the app, not the eraser.
        tablet.away()
        time.sleep(0.3)
        run("xdotool", "key", "Tab")
        time.sleep(0.3)
        shot(out, "6-tab-not-taken")

        # 7. Drag the Popup to move it, then by its corner to resize it.
        x, y, w, h = popup_geometry()
        drag((x + w // 2, y + h // 2), (x + w // 2 + 300, y + h // 2 + 150))
        report["after move"] = popup_geometry()
        x, y, w, h = popup_geometry()
        drag((x + w - 6, y + h - 6), (x + w - 6 - 240, y + h - 6 - 50))
        report["after resize"] = popup_geometry()
        report["remembered"] = (out / "state" / "decopad" / "popup").read_text().strip()
        shot(out, "7-moved-resized")

        # 8. Popup Toggle hides it and reopens it where it was left.
        run("xdotool", "key", "super+w")
        time.sleep(0.4)
        report["hidden"] = popup_geometry() is None
        run("xdotool", "key", "super+w")
        time.sleep(0.4)
        report["reopened"] = popup_geometry()

        # 9. Commit where the Popup covers the target: it steps aside.
        x, y, w, h = popup_geometry()
        run("xdotool", "mousemove", str(x + 60), str(y + 80))
        tablet.hover(4000, 3000)
        write_hi(tablet, 1500, 3000)
        tablet.express_key([e.KEY_E])
        hidden = False
        for _ in range(40):
            hidden |= popup_geometry() is None
            time.sleep(0.02)
        report["hidden during covered replay"] = hidden
        time.sleep(2.0)
        report["back after covered replay"] = popup_geometry()
        run("xdotool", "key", "super+w")
        time.sleep(0.3)
        shot(out, "9-covered-replay")
        run("xdotool", "key", "super+w")
        time.sleep(0.3)

        # 10. Undo (key 7, factory Ctrl+Z) and the eraser (Tab while writing).
        tablet.hover(4000, 3000)
        tablet.stroke(line((1500, 2000), (4000, 2000)))
        tablet.stroke(line((1500, 4500), (4000, 4500)))
        tablet.stroke(line((1500, 6000), (4000, 6000)))
        tablet.express_key([e.KEY_LEFTCTRL, e.KEY_Z])
        tablet.hover(4000, 3000)
        run("xdotool", "key", "Tab")
        time.sleep(0.2)
        tablet.stroke(line((2500, 1500), (2500, 2500)))
        shot(out, "10-undo-erase")
        run("xdotool", "key", "Tab")
        run("xdotool", "key", "BackSpace")

        # 10b. Pen buttons in Pad Mode: lower undoes a stroke, upper redoes it.
        tablet.stroke(line((1500, 2000), (4000, 2000)))
        tablet.stroke(line((1500, 4500), (4000, 4500)))
        tablet.button(e.BTN_STYLUS)
        shot(out, "10b-pen-undo")
        tablet.upper_button()
        shot(out, "10c-pen-redo")
        # 10d. Holding the upper button while writing erases, and releasing
        # it afterwards does not redo.
        tablet.upper_down()
        for i, (x, y) in enumerate(line((2500, 4000), (2500, 5000))):
            tablet.pen.write(e.EV_ABS, e.ABS_X, int(x))
            tablet.pen.write(e.EV_ABS, e.ABS_Y, int(y))
            tablet.pen.write(e.EV_ABS, e.ABS_PRESSURE, 6000)
            if i == 0:
                tablet.pen.write(e.EV_KEY, e.BTN_TOUCH, 1)
            tablet.pen.syn()
            time.sleep(0.004)
        tablet.pen.write(e.EV_ABS, e.ABS_PRESSURE, 0)
        tablet.pen.write(e.EV_KEY, e.BTN_TOUCH, 0)
        tablet.pen.syn()
        time.sleep(0.05)
        tablet.upper_up()
        shot(out, "10d-hold-erase")
        run("xdotool", "key", "BackSpace")

        # 11. Ink Scale: key 6 (Ctrl+KP+) twice, key 5 (Ctrl+KP-) once.
        for combo in ([e.KEY_LEFTCTRL, e.KEY_KPPLUS], [e.KEY_LEFTCTRL, e.KEY_KPPLUS], [e.KEY_LEFTCTRL, e.KEY_KPMINUS]):
            tablet.express_key(combo)
        report["ink scale"] = (out / "state" / "decopad" / "ink-scale").read_text().strip()
        shot(out, "11-ink-scale")
        for combo in ([e.KEY_LEFTCTRL, e.KEY_KPMINUS],):
            tablet.express_key(combo)

        # 12. Saving the config reloads it: a pause now commits by itself.
        config = out / "config" / "decopad" / "config.toml"
        config.write_text(config_text.replace("commit-delay-ms = 0", "commit-delay-ms = 800"))
        time.sleep(0.5)
        run("xdotool", "mousemove", *map(str, (anchor[0], anchor[1] + 200)))
        tablet.hover(4000, 3000)
        write_hi(tablet, 1500, 3000)
        tablet.away()
        time.sleep(0.4)
        report["before pause commit"] = pointer()
        time.sleep(2.5)
        report["after pause commit"] = pointer()

        # 13. Esc while writing closes the Popup and keeps its Ink.
        tablet.hover(4000, 3000)
        tablet.stroke(line((1500, 6000), (6000, 6000)))
        run("xdotool", "key", "Escape")
        time.sleep(0.4)
        report["closed by Esc"] = popup_geometry() is None
        tablet.away()

        # 14. With the Popup closed (Desktop Mode) the Pen drives the Pointer
        # (Tablet centre is screen centre); key 1 reopens it with the kept
        # Ink, and then the Pen no longer moves the Pointer.
        tablet.hover(8000, 4500)
        tablet.hover(8001, 4501)
        report["desktop mode pointer"] = pointer()
        report["X pen buttons off"] = [run("xsetwacom", "get", "decopad-test Pen stylus", "Button", b) for b in "23"]
        report["X eraser device"] = [l.split("\t")[-1].strip() for l in run("xinput", "list").splitlines() if "decopad-test Pen eraser" in l]
        tablet.away()
        tablet.express_key([e.KEY_B])
        run("xdotool", "mousemove", "300", "300")
        tablet.hover(12000, 7000)
        report["pad mode pointer"] = pointer()
        report["popup reopened by key 1"] = popup_geometry() is not None
        shot(out, "13-kept-ink")
        tablet.away()
        shot(out, "12-end")
        for key, value in report.items():
            print(f"{key}: {value}")
    finally:
        decopad.terminate()
        decopad.wait()
        xournal.kill()
        tablet.keyboard.close()
        tablet.pen.close()


if __name__ == "__main__":
    main()
