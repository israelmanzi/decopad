# 0001 Rust with a pure-Rust X11 stack

**Date:** 2026-10-17
**Status:** Accepted

## Context
The software runs permanently in the background: it reads the Tablet at about
200 reports per second, owns the Express Keys, draws Ink in the Popup and
drives a virtual pen for Replay. It targets one machine (Fedora 43, XFCE on
X11). The user wants it to feel instant and stay light while idle.

## Decision
Write it in Rust as one binary, using crates that talk to the kernel and the
X server directly, without C toolkit bindings:

- `evdev` for reading and grabbing the Tablet's nodes and for uinput devices.
- `x11rb` for the Popup window, the Popup Toggle key grab, Pointer queries,
  clipboard ownership and Desktop Mode area mapping.
- `tiny-skia` for antialiased Ink rendering, with a TrueType rasteriser for the
  Cheatsheet text.

## Alternatives considered
- **Python with GTK3:** everything was already installed and it is faster to
  iterate on, but the user prioritised responsiveness and a low idle cost.
- **Rust with GTK4 bindings:** needs the `gtk4-devel` headers, and GTK4 no
  longer exposes always-on-top or override-redirect windows, which the Popup
  needs.
- **Rust with winit/egui:** pulls in a GPU stack for a single small window, and
  still needs raw X11 calls for key grabs, Pointer queries and device mapping.

## Consequences
- No system `-devel` packages are needed to build, and the result is a single
  binary.
- Window behaviour, text layout and clipboard handling are written by hand
  against X11, so the code is more verbose than with a toolkit.
- Moving to Wayland later would mean replacing the `x11rb` layer.
