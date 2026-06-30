#!/usr/bin/env bash
#
# Assembles Glance.app from the release binary, with an ad-hoc code signature.
# No Xcode project required — just the Command Line Tools.
#
# Usage:
#   scripts/bundle.sh             build + bundle into target/release/bundle/
#   scripts/bundle.sh --install   also copy Glance.app into /Applications
#
set -euo pipefail

APP_NAME="Glance"
BIN_NAME="glance"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="$ROOT/target/release/$BIN_NAME"
APP_DIR="$ROOT/target/release/bundle/$APP_NAME.app"
CONTENTS="$APP_DIR/Contents"

echo "==> Building release binary"
cargo build --release --manifest-path "$ROOT/Cargo.toml"

echo "==> Assembling $APP_NAME.app"
rm -rf "$APP_DIR"
mkdir -p "$CONTENTS/MacOS" "$CONTENTS/Resources"
cp "$BIN" "$CONTENTS/MacOS/$BIN_NAME"
cp "$ROOT/bundle/Info.plist" "$CONTENTS/Info.plist"
if [ -f "$ROOT/bundle/$APP_NAME.icns" ]; then
  cp "$ROOT/bundle/$APP_NAME.icns" "$CONTENTS/Resources/$APP_NAME.icns"
else
  echo "    (no bundle/$APP_NAME.icns — using the generic app icon)"
fi

echo "==> Ad-hoc code signing"
# Ad-hoc ("-") gives a working signature for local use. For permissions that
# survive rebuilds, sign with a self-signed certificate instead (see README).
codesign --force --sign - "$APP_DIR"

echo "==> Built: $APP_DIR"

if [ "${1:-}" = "--install" ]; then
  echo "==> Installing to /Applications"
  rm -rf "/Applications/$APP_NAME.app"
  cp -R "$APP_DIR" "/Applications/$APP_NAME.app"
  echo "==> Installed: /Applications/$APP_NAME.app"
fi
