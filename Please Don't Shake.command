#!/bin/bash
#
# Double-click to play.
#
#   click            tap the glass
#   click and drag   shake the tank
#   right-drag       dig by hand   (M1 debug — the ants' job in M2)
#   shift+right-drag fill sand back in
#   F12              save a screenshot next to this file
#   [ and ]          colony speed: real time / a day an hour / a minute / a second
#
# The speed keys move biology only — brood, ageing, laying. The sand and the ants are
# always real time. Every run starts at real time; nothing remembers the speed.
#
# Pass --speed to start fast, where 1 is real time and 86400 is a colony day a second:
#   ./"Please Don't Shake.command" --speed 1440
#
# Pass --capture to run the scripted dig/tap/shake verification instead:
#   ./"Please Don't Shake.command" --capture --out /tmp/shots

# Finder launches .command files from the user's home directory, so step into the
# project before doing anything else.
cd "$(dirname "$0")" || exit 1

# A GUI-launched shell doesn't necessarily have rustup's PATH set up.
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
command -v cargo >/dev/null || PATH="$HOME/.cargo/bin:$PATH"

if ! command -v cargo >/dev/null; then
    echo "Couldn't find cargo. Install Rust from https://rustup.rs and try again."
    echo
    echo "Press any key to close."
    read -n 1 -s
    exit 1
fi

# Near-instant when nothing has changed, so this always runs current code.
echo "Building…"
if ! cargo build --release; then
    echo
    echo "Build failed — the error is above. Press any key to close."
    read -n 1 -s
    exit 1
fi

clear
exec ./target/release/please_dont_shake "$@"
