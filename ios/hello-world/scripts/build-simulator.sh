#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

if ! command -v xcodegen >/dev/null 2>&1; then
  echo "xcodegen is required. Install with: brew install xcodegen" >&2
  exit 1
fi

cd "$PROJECT_ROOT"
xcodegen generate --spec project.yml

xcodebuild \
  -project GPUIiOSHello.xcodeproj \
  -scheme GPUIiOSHello \
  -configuration Debug \
  -destination "generic/platform=iOS Simulator" \
  -derivedDataPath "$PROJECT_ROOT/build/simulator" \
  build

echo "Simulator build succeeded."
