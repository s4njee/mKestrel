#!/usr/bin/env bash
# Bundle mkestral for the iOS simulator and install + launch it.
#
# Usage: scripts/bundle-ios-sim.sh [device-udid] [debug|release]
#   device-udid  default "booted" (the currently booted simulator).
#   profile      default "debug". "release" seeds no fixtures (real-only hosts).
#
# This is the known-good recipe recorded in docs/dev-setup.md (E0-S2). The
# interactive dev loop (`dx serve --platform ios`) is documented alongside.
set -euo pipefail

cd "$(dirname "$0")/.."

TARGET="aarch64-apple-ios-sim"
PROFILE="${2:-debug}"
APP_NAME="mkestral"
BUNDLE_ID="com.mkestral.app"
UDID="${1:-booted}"

FLAG=""
if [[ "${PROFILE}" == "release" ]]; then
  FLAG="--release"
fi

echo "→ cargo build --target ${TARGET} ${FLAG} (no-default-features, features mobile, ${PROFILE})…"
cargo build --target "${TARGET}" ${FLAG} --no-default-features --features mobile -p "${APP_NAME}"

STAGE="target/${TARGET}/${PROFILE}/${APP_NAME}.app"
rm -rf "${STAGE}"
mkdir -p "${STAGE}"
cp "target/${TARGET}/${PROFILE}/${APP_NAME}" "${STAGE}/${APP_NAME}"

cat > "${STAGE}/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key><string>en</string>
  <key>CFBundleExecutable</key><string>${APP_NAME}</string>
  <key>CFBundleIdentifier</key><string>${BUNDLE_ID}</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>CFBundleName</key><string>mkestral</string>
  <key>CFBundleDisplayName</key><string>mkestral</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>LSRequiresIPhoneOS</key><true/>
  <key>MinimumOSVersion</key><string>15.0</string>
  <key>UIDeviceFamily</key><array><integer>1</integer><integer>2</integer></array>
  <key>UILaunchScreen</key><dict/>
  <key>UIRequiredDeviceCapabilities</key><array><string>arm64</string></array>
  <key>UISupportedInterfaceOrientations</key>
  <array>
    <string>UIInterfaceOrientationPortrait</string>
    <string>UIInterfaceOrientationLandscapeLeft</string>
    <string>UIInterfaceOrientationLandscapeRight</string>
  </array>
  <key>UISupportedInterfaceOrientations~ipad</key>
  <array>
    <string>UIInterfaceOrientationPortrait</string>
    <string>UIInterfaceOrientationPortraitUpsideDown</string>
    <string>UIInterfaceOrientationLandscapeLeft</string>
    <string>UIInterfaceOrientationLandscapeRight</string>
  </array>
</dict>
</plist>
PLIST

echo "→ ad-hoc signing ${STAGE}…"
codesign --force --sign - "${STAGE}"

echo "→ installing on ${UDID}…"
xcrun simctl install "${UDID}" "${STAGE}"

echo "→ launching ${BUNDLE_ID}…"
xcrun simctl launch "${UDID}" "${BUNDLE_ID}"
