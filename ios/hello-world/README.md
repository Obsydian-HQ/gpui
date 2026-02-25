# GPUI iOS Hello World

This iOS host app boots the real `gpui` iOS platform implementation via
`gpui_ios_app` (Rust static library).

## Prerequisites

- Xcode 15+ (for `xcrun devicectl`)
- `xcodegen` (`brew install xcodegen`)
- Rust iOS targets (scripts install them automatically as needed)
- A connected physical iPhone/iPad with Developer Mode enabled

## Commands

List connected devices:

```bash
./scripts/list-devices.sh
```

Build for iOS simulator:

```bash
./scripts/build-simulator.sh
```

Build, install, and launch on physical device:

```bash
./scripts/run-device.sh
```

Optional: force a specific device UDID or team id:

```bash
DEVELOPMENT_TEAM=DA7B5U47PT ./scripts/run-device.sh <DEVICE_UDID>
```

If launch fails with a "device locked" message, unlock the phone and rerun the
device script.
