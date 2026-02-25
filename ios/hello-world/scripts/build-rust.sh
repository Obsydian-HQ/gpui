#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
WORKSPACE_ROOT="$(cd "${PROJECT_ROOT}/../.." && pwd)"

PROFILE_DIR="debug"
CARGO_RELEASE_FLAG=""
if [[ "${CONFIGURATION:-Debug}" == "Release" ]]; then
  PROFILE_DIR="release"
  CARGO_RELEASE_FLAG="--release"
fi

build_rust_target() {
  local target="$1"
  rustup target add "$target" >/dev/null 2>&1 || true
  cd "$WORKSPACE_ROOT"
  if [[ -n "$CARGO_RELEASE_FLAG" ]]; then
    cargo build -p gpui_ios_app --target "$target" --release
  else
    cargo build -p gpui_ios_app --target "$target"
  fi
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
    TARGET_X64="x86_64-apple-ios"
    build_rust_target "$TARGET_ARM64"
    build_rust_target "$TARGET_X64"
    ARM64_LIB="$(rust_lib_path "$TARGET_ARM64")"
    X64_LIB="$(rust_lib_path "$TARGET_X64")"
    if [[ ! -f "$ARM64_LIB" || ! -f "$X64_LIB" ]]; then
      echo "Missing simulator Rust static libraries." >&2
      echo "  arm64: $ARM64_LIB" >&2
      echo "  x86_64: $X64_LIB" >&2
      exit 1
    fi
    lipo -create -output "$BUILT_PRODUCTS_DIR/libgpui_ios_app.a" "$ARM64_LIB" "$X64_LIB"
    ;;
  *)
    echo "Unsupported PLATFORM_NAME=${PLATFORM_NAME:-unknown}" >&2
    exit 1
    ;;
esac
