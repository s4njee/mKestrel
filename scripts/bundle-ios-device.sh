#!/usr/bin/env bash
# Build, sign, and install mkestral on a physical iPad (device target).
#
# Signing uses the existing development identity + the `io.mk7s.app` dev
# provisioning profile (team CBCWMTN2X2), which already registers this iPad's
# UDID. Usage: scripts/bundle-ios-device.sh [device-udid]
set -euo pipefail
cd "$(dirname "$0")/.."

TARGET="aarch64-apple-ios"
PROFILE="${RELEASE:+release}"
PROFILE="${PROFILE:-debug}"
APP_NAME="mkestral"
BUNDLE_ID="io.mk7s.app"
TEAM="CBCWMTN2X2"
IDENTITY="Apple Development: sanjee.yogeswaran@gmail.com (BQ59W7239A)"
PROVISION="$HOME/Library/MobileDevice/Provisioning Profiles/mk7s-io.mk7s.app-dev.mobileprovision"
UDID="${1:-00008112-000858E10A83A01E}"

echo "→ cargo build --target ${TARGET} (${PROFILE})…"
BUILD_EXTRA=()
[ "${RELEASE}" = "1" ] && BUILD_EXTRA+=(--release)
cargo build --target "${TARGET}" --no-default-features --features mobile -p "${APP_NAME}" "${BUILD_EXTRA[@]}"

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
</dict>
</plist>
PLIST

cat > "${STAGE}/entitlements.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>application-identifier</key><string>${TEAM}.${BUNDLE_ID}</string>
  <key>com.apple.developer.team-identifier</key><string>${TEAM}</string>
  <key>get-task-allow</key><true/>
</dict></plist>
PLIST

# Embed the provisioning profile + sign with the dev identity.
cp "${PROVISION}" "${STAGE}/embedded.mobileprovision"
codesign --force --sign "${IDENTITY}" \
  --entitlements "${STAGE}/entitlements.plist" \
  --identifier "${BUNDLE_ID}" \
  "${STAGE}"

echo "→ installing on ${UDID}…"
xcrun devicectl device install app --device "${UDID}" "${STAGE}"
echo "→ installed. Launch with: xcrun devicectl device process launch --device ${UDID} ${BUNDLE_ID}"
