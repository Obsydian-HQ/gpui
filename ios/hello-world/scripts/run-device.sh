#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
TEAM_ID="${DEVELOPMENT_TEAM:-DA7B5U47PT}"
BUNDLE_ID="dev.glasshq.GPUIiOSHello"
LOG_PORT="${GPUI_LOG_PORT:-9632}"

# ---------------------------------------------------------------------------
# Prerequisites
# ---------------------------------------------------------------------------
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

# ---------------------------------------------------------------------------
# Device selection
#
# xcodebuild and devicectl use DIFFERENT identifiers for the same device:
#   - xcodebuild -destination "id=..." expects the legacy UDID
#     (e.g., 00008110-001461262E09A01E)
#   - devicectl --device expects the CoreDevice UUID
#     (e.g., 1B4A45C4-F142-569D-B798-3FBBE81CD0F0)
#
# We query devicectl JSON to get both, then use each where appropriate.
# ---------------------------------------------------------------------------
TMPJSON="$(mktemp /tmp/gpui-devices.XXXXXX.json)"
xcrun devicectl list devices --json-output "$TMPJSON" >/dev/null 2>&1 || true

# Auto-select: prefer devices with active tunnel, then recently-connected,
# then any known physical iOS device.
read -r COREDEVICE_ID LEGACY_UDID < <(python3 -c "
import json
data = json.load(open('$TMPJSON'))
devices = data.get('result', {}).get('devices', [])
active = []
recent = []
offline = []
for d in devices:
    hw = d.get('hardwareProperties', {})
    if hw.get('reality') == 'physical' and hw.get('platform') == 'iOS':
        pair = (d.get('identifier', ''), hw.get('udid', ''))
        tunnel = d.get('connectionProperties', {}).get('tunnelState', 'unavailable')
        if tunnel not in ('unavailable', 'disconnected'):
            active.append(pair)
        elif tunnel == 'disconnected':
            recent.append(pair)
        else:
            offline.append(pair)
pick = active or recent or offline
if pick:
    print(pick[0][0], pick[0][1])
" 2>/dev/null || echo "")
rm -f "$TMPJSON"

# Allow explicit override via $1 (tries as UDID for xcodebuild).
if [[ -n "${1:-}" ]]; then
  COREDEVICE_ID="${1}"
  LEGACY_UDID="${1}"
fi

if [[ -z "$COREDEVICE_ID" || -z "$LEGACY_UDID" ]]; then
  echo "No physical iOS device found. Pair a device first:" >&2
  echo "  Settings → General → VPN & Device Management, or plug in via USB." >&2
  exit 1
fi

echo "Using device: ${LEGACY_UDID} (CoreDevice: ${COREDEVICE_ID})"
echo "Using development team: $TEAM_ID"

# ---------------------------------------------------------------------------
# Start the log listener BEFORE building so it's ready when the app launches.
# The iOS app connects to this TCP socket and streams log lines over Wi-Fi.
# ---------------------------------------------------------------------------
LOG_LISTENER_PID=""

cleanup() {
  if [[ -n "$LOG_LISTENER_PID" ]]; then
    kill "$LOG_LISTENER_PID" 2>/dev/null || true
    wait "$LOG_LISTENER_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT INT TERM

# Start a persistent TCP listener. Each accepted connection is piped to stdout.
# `socat` gives the best experience (auto-reconnect on app relaunch).
# -u = unidirectional (TCP → stdout only, no read-back).
if command -v socat >/dev/null 2>&1; then
  socat -u TCP-LISTEN:"$LOG_PORT",reuseaddr,fork STDOUT &
  LOG_LISTENER_PID=$!
else
  # macOS nc: -l (listen), -k (keep listening after disconnect)
  nc -l -k "$LOG_PORT" &
  LOG_LISTENER_PID=$!
fi

echo "Log listener started on port $LOG_PORT (PID $LOG_LISTENER_PID)"

# ---------------------------------------------------------------------------
# Build — xcodebuild needs the legacy UDID
# ---------------------------------------------------------------------------
cd "$PROJECT_ROOT"
xcodegen generate --spec project.yml

xcodebuild \
  -project GPUIiOSHello.xcodeproj \
  -scheme GPUIiOSHello \
  -configuration Debug \
  -destination "id=$LEGACY_UDID" \
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

# ---------------------------------------------------------------------------
# Install & Launch — devicectl needs the CoreDevice UUID
# ---------------------------------------------------------------------------
echo "Installing app..."
xcrun devicectl device install app --device "$COREDEVICE_ID" "$APP_PATH"

echo ""
echo "Launching app..."
set +e
launch_output="$(xcrun devicectl device process launch --terminate-existing --device "$COREDEVICE_ID" "$BUNDLE_ID" 2>&1)"
launch_status=$?
set -e

if [[ "$launch_status" -ne 0 ]]; then
  echo "$launch_output"
  if echo "$launch_output" | grep -qi "locked"; then
    echo "" >&2
    echo "Device is locked. Unlock your phone and try again." >&2
  fi
  exit "$launch_status"
fi

echo "$launch_output"
echo ""
echo "--- Streaming logs (Ctrl+C to stop) ---"
echo ""

# Wait for the log listener to be interrupted (Ctrl+C) or the app to disconnect.
wait "$LOG_LISTENER_PID" 2>/dev/null || true
LOG_LISTENER_PID=""
