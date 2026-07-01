#!/usr/bin/env bash
#
# Generates bundle/Glance.icns from an emoji, using only the Command Line Tools
# (swift + sips + iconutil). Re-run scripts/bundle.sh afterwards to embed it.
#
# Usage:
#   scripts/make_icon.sh            # uses the default emoji
#   scripts/make_icon.sh 🚀         # any emoji
#
set -euo pipefail

EMOJI="${1:-🧿}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# 1) Render the emoji onto a rounded card at 1024×1024.
cat > "$WORK/render.swift" <<'SWIFT'
import AppKit

let emoji = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "🧿"
let out = CommandLine.arguments.count > 2 ? CommandLine.arguments[2] : "icon.png"
let px = 1024

let rep = NSBitmapImageRep(
    bitmapDataPlanes: nil, pixelsWide: px, pixelsHigh: px,
    bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
    colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!

NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)

let size = CGFloat(px)
let inset = size * 0.06
let card = NSRect(x: inset, y: inset, width: size - 2*inset, height: size - 2*inset)
let radius = card.width * 0.2237 // macOS squircle-ish
// Background card color — edit to taste.
NSColor(calibratedRed: 0.11, green: 0.11, blue: 0.15, alpha: 1).setFill()
NSBezierPath(roundedRect: card, xRadius: radius, yRadius: radius).fill()

let para = NSMutableParagraphStyle()
para.alignment = .center
let str = NSAttributedString(string: emoji, attributes: [
    .font: NSFont.systemFont(ofSize: size * 0.6),
    .paragraphStyle: para,
])
let s = str.size()
str.draw(in: NSRect(x: (size - s.width)/2, y: (size - s.height)/2, width: s.width, height: s.height))

NSGraphicsContext.restoreGraphicsState()

let png = rep.representation(using: .png, properties: [:])!
try! png.write(to: URL(fileURLWithPath: out))
SWIFT

echo "==> Rendering $EMOJI"
swift "$WORK/render.swift" "$EMOJI" "$WORK/icon.png"

# 2) Slice into the required iconset sizes.
ICONSET="$WORK/Glance.iconset"
mkdir -p "$ICONSET"
for size in 16 32 128 256 512; do
  sips -z "$size" "$size" "$WORK/icon.png" --out "$ICONSET/icon_${size}x${size}.png" >/dev/null
  double=$((size * 2))
  sips -z "$double" "$double" "$WORK/icon.png" --out "$ICONSET/icon_${size}x${size}@2x.png" >/dev/null
done

# 3) Build the .icns.
mkdir -p "$ROOT/bundle"
iconutil -c icns "$ICONSET" -o "$ROOT/bundle/Glance.icns"
echo "==> Wrote $ROOT/bundle/Glance.icns"
