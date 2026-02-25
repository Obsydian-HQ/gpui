#!/usr/bin/env bash
set -euo pipefail

if ! xcrun devicectl list devices --help >/dev/null 2>&1; then
  echo "xcrun devicectl is required (Xcode 15+)." >&2
  exit 1
fi

TMPJSON="$(mktemp /tmp/gpui-devices.XXXXXX.json)"
xcrun devicectl list devices --json-output "$TMPJSON" >/dev/null 2>&1 || true

echo "Connected iOS devices:"
python3 -c "
import json, sys
data = json.load(open('$TMPJSON'))
devices = data.get('result', {}).get('devices', [])
found = False
for d in devices:
    hw = d.get('hardwareProperties', {})
    if hw.get('reality') == 'physical' and hw.get('platform') == 'iOS':
        ident = d.get('identifier', '')
        name = d.get('deviceProperties', {}).get('name', 'unknown')
        model = hw.get('marketingName', 'unknown')
        os_ver = d.get('deviceProperties', {}).get('osVersionNumber', '?')
        tunnel = d.get('connectionProperties', {}).get('tunnelState', 'unavailable')
        if tunnel == 'unavailable':
            status = 'unavailable'
        elif tunnel == 'disconnected':
            status = 'reachable'
        else:
            status = 'available'
        print(f'  {ident}\t{name}\t{model}\tiOS {os_ver}\t({status})')
        found = True
if not found:
    print('  (none found)')
    sys.exit(1)
"
STATUS=$?
rm -f "$TMPJSON"
exit $STATUS
