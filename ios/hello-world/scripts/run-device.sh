#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
LIST_SCRIPT="${SCRIPT_DIR}/list-devices.sh"
TEAM_ID="${DEVELOPMENT_TEAM:-DA7B5U47PT}"
BUNDLE_ID="dev.glasshq.GPUIiOSHello"

if ! command -v xcodegen >/dev/null 2>&1; then
  echo "xcodegen is required. Install with: brew install xcodegen" >&2
  exit 1
fi

if ! command -v xcrun >/dev/null 2>&1; then
  echo "xcrun is required (install Xcode command line tools)." >&2
  exit 1
fi

if ! xcrun devicectl --help >/dev/null 2>&1; then
  echo "xcrun devicectl is required (Xcode 15+)." >&2
  exit 1
fi

DEVICE_ID="${1:-}"
if [[ -z "$DEVICE_ID" ]]; then
  DEVICE_ID="$("$LIST_SCRIPT" | awk 'NR==2 {print $1}')"
fi

if [[ -z "$DEVICE_ID" ]]; then
  echo "No physical iOS device found." >&2
  exit 1
fi

echo "Using device: $DEVICE_ID"
echo "Using development team: $TEAM_ID"

cd "$PROJECT_ROOT"
xcodegen generate --spec project.yml

xcodebuild \
  -project GPUIiOSHello.xcodeproj \
  -scheme GPUIiOSHello \
  -configuration Debug \
  -destination "id=$DEVICE_ID" \
  -derivedDataPath "$PROJECT_ROOT/build/device" \
  -allowProvisioningUpdates \
  DEVELOPMENT_TEAM="$TEAM_ID" \
  CODE_SIGN_STYLE=Automatic \
  build

APP_PATH="$PROJECT_ROOT/build/device/Build/Products/Debug-iphoneos/GPUIiOSHello.app"
if [[ ! -d "$APP_PATH" ]]; then
  echo "Built app bundle not found: $APP_PATH" >&2
  exit 1
fi

echo "Installing app..."
xcrun devicectl device install app --device "$DEVICE_ID" "$APP_PATH"

echo "Launching app..."
set +e
launch_output="$(xcrun devicectl device process launch --device "$DEVICE_ID" "$BUNDLE_ID" 2>&1)"
launch_status=$?
set -e
echo "$launch_output"

if [[ "$launch_status" -ne 0 ]]; then
  if echo "$launch_output" | grep -Eq "could not be, unlocked|BSErrorCodeDescription = Locked"; then
    echo "Launch failed because the device is locked. Unlock the phone and rerun this script." >&2
  fi
  exit "$launch_status"
fi

echo "Device smoke launch completed."
