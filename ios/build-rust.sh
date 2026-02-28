#!/usr/bin/env bash
# Called by xcodebuild's pre-build step. Compiles the Rust static library
# for the target platform and copies it into $BUILT_PRODUCTS_DIR.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

PROFILE_DIR="debug"
CARGO_RELEASE_FLAG=""
if [[ "${CONFIGURATION:-Debug}" == "Release" ]]; then
  PROFILE_DIR="release"
  CARGO_RELEASE_FLAG="--release"
fi

# ---------------------------------------------------------------------------
# Log relay: detect the Mac's local IP so the iOS app can stream logs back
# over Wi-Fi (TCP port 9632).
# ---------------------------------------------------------------------------
LOG_PORT="${GPUI_LOG_PORT:-9632}"
if [[ -z "${GPUI_LOG_RELAY:-}" ]]; then
  HOST_IP=""
  for iface in en0 en1 en2 en3 en4; do
    HOST_IP="$(ipconfig getifaddr "$iface" 2>/dev/null || true)"
    if [[ -n "$HOST_IP" ]]; then
      break
    fi
  done
  if [[ -n "$HOST_IP" ]]; then
    export GPUI_LOG_RELAY="${HOST_IP}:${LOG_PORT}"
  fi
fi

if [[ -n "${GPUI_LOG_RELAY:-}" ]]; then
  echo "Log relay target: ${GPUI_LOG_RELAY}"
else
  echo "Log relay: disabled (no local network IP detected)"
fi

build_rust_target() {
  local target="$1"
  rustup target add "$target" >/dev/null 2>&1 || true
  cd "$WORKSPACE_ROOT"
  cargo build -p gpui_ios_app --target "$target" $CARGO_RELEASE_FLAG
}

rust_lib_path() {
  local target="$1"
  echo "$WORKSPACE_ROOT/target/$target/$PROFILE_DIR/libgpui_ios_app.a"
}

case "${PLATFORM_NAME:-}" in
  iphoneos)
    TARGET="aarch64-apple-ios"
    build_rust_target "$TARGET"
    DEVICE_LIB="$(rust_lib_path "$TARGET")"
    if [[ ! -f "$DEVICE_LIB" ]]; then
      echo "Missing Rust static library: $DEVICE_LIB" >&2
      exit 1
    fi
    cp "$DEVICE_LIB" "$BUILT_PRODUCTS_DIR/libgpui_ios_app.a"
    ;;
  iphonesimulator)
    TARGET_ARM64="aarch64-apple-ios-sim"
    build_rust_target "$TARGET_ARM64"
    ARM64_LIB="$(rust_lib_path "$TARGET_ARM64")"

    HOST_ARCH="$(uname -m)"
    if [[ "$HOST_ARCH" == "arm64" ]]; then
      if [[ ! -f "$ARM64_LIB" ]]; then
        echo "Missing Rust static library: $ARM64_LIB" >&2
        exit 1
      fi
      cp "$ARM64_LIB" "$BUILT_PRODUCTS_DIR/libgpui_ios_app.a"
    else
      TARGET_X64="x86_64-apple-ios"
      build_rust_target "$TARGET_X64"
      X64_LIB="$(rust_lib_path "$TARGET_X64")"
      if [[ ! -f "$ARM64_LIB" || ! -f "$X64_LIB" ]]; then
        echo "Missing simulator Rust static libraries." >&2
        exit 1
      fi
      lipo -create -output "$BUILT_PRODUCTS_DIR/libgpui_ios_app.a" "$ARM64_LIB" "$X64_LIB"
    fi
    ;;
  *)
    echo "Unsupported PLATFORM_NAME=${PLATFORM_NAME:-unknown}" >&2
    exit 1
    ;;
esac
