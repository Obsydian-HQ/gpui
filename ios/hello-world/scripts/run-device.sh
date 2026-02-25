#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
LIST_SCRIPT="${SCRIPT_DIR}/list-devices.sh"
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
# ---------------------------------------------------------------------------
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
# Build
# ---------------------------------------------------------------------------
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

# ---------------------------------------------------------------------------
# Install & Launch
# ---------------------------------------------------------------------------
echo "Installing app..."
xcrun devicectl device install app --device "$DEVICE_ID" "$APP_PATH"

echo ""
echo "Launching app..."
set +e
launch_output="$(xcrun devicectl device process launch --terminate-existing --device "$DEVICE_ID" "$BUNDLE_ID" 2>&1)"
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
