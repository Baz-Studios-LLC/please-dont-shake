#!/usr/bin/env bash
# Assemble PleaseDontShake.app around an already-built release binary.
# Usage: packaging/macos-app.sh <path-to-binary> <version> [out-dir]
#
# Like Divus Factus, this game loads real files out of assets/ at runtime — the
# sand's theme, the room behind the glass, the music, the sticker — so the bundle
# carries the assets folder BESIDE the binary. `asset_root()` in src/main.rs looks
# next to the executable first, which is what makes that work.
#
# The bundle is named without a space on purpose. Finder shows the spaced name from
# CFBundleDisplayName anyway, and a spaced path — let alone the apostrophe in this
# game's actual title — is one more thing for tar, codesign and the launcher to get
# right. The launcher finds the bundle by its `.app` extension rather than by name.
set -euo pipefail
BIN="${1:?usage: macos-app.sh <binary> <version> [out-dir]}"
VERSION="${2:?need a version}"
OUT="${3:-dist}"
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"

APP="$OUT/PleaseDontShake.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

# The executable's name must match CFBundleExecutable in Info.plist, or macOS
# refuses to launch the bundle.
cp "$BIN" "$APP/Contents/MacOS/please-dont-shake"
chmod +x "$APP/Contents/MacOS/please-dont-shake"
# A Bevy release binary is ~100MB unstripped. Stripping keeps the bundle small
# enough that hdiutil has scratch space left for the .dmg.
strip "$APP/Contents/MacOS/please-dont-shake" 2>/dev/null || true

cp -R "$ROOT/assets" "$APP/Contents/MacOS/assets"

# CFBundleIconFile names it without the extension.
cp "$HERE/PleaseDontShake.icns" "$APP/Contents/Resources/PleaseDontShake.icns"
sed "s/__VERSION__/$VERSION/g" "$HERE/Info.plist" > "$APP/Contents/Info.plist"

# Ad-hoc sign so macOS runs it instead of calling it damaged; the launcher also
# strips the download quarantine on install.
codesign --force --deep --sign - "$APP" 2>/dev/null || true

echo "built $APP ($(du -sh "$APP" | cut -f1))"
