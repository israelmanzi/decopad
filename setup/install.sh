#!/bin/sh
# Installs decopad for the current user: device access (needs sudo), the
# binary, and an XFCE/XDG autostart entry. Safe to run again to update.
set -eu
cd "$(dirname "$0")/.."

echo "== Device access (sudo)"
sudo install -m644 setup/70-xppen-deco640.rules /etc/udev/rules.d/
sudo install -m644 setup/uinput.conf /etc/modules-load.d/
sudo modprobe uinput
sudo udevadm control --reload
sudo udevadm trigger

echo "== Build and install"
cargo install --locked --path decopad

echo "== Autostart"
install -Dm644 setup/decopad.desktop "$HOME/.config/autostart/decopad.desktop"

echo "== (Re)start"
pkill -x decopad 2>/dev/null || true
while pgrep -x decopad >/dev/null; do sleep 0.2; done
setsid -f sh -c 'exec "$HOME/.cargo/bin/decopad" 2>>"$HOME/.cache/decopad.log"' </dev/null
echo "decopad is running; log: ~/.cache/decopad.log"
