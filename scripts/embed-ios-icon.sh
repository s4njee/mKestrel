#!/usr/bin/env bash
# Compile assets/icon/Assets.xcassets into $1 (the .app) as Assets.car.
# Usage: scripts/embed-ios-icon.sh <app-dir> <iphonesimulator|iphoneos>
set -euo pipefail
APP="${1:?app dir}"
PLATFORM="${2:?iphonesimulator or iphoneos}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
XC="$ROOT/assets/icon/Assets.xcassets"
PARTIAL="$APP/icon-partial.plist"

xcrun actool "$XC" \
  --compile "$APP" \
  --platform "$PLATFORM" \
  --minimum-deployment-target 15.0 \
  --app-icon AppIcon \
  --output-partial-info-plist "$PARTIAL" \
  --notices --warnings

# Fall back: also drop the 1024px master so CFBundleIconFile can find it.
cp "$ROOT/assets/icon/icon.png" "$APP/AppIcon.png"
rm -f "$PARTIAL"
