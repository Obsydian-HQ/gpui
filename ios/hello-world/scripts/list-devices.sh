#!/usr/bin/env bash
set -euo pipefail

if ! command -v xcrun >/dev/null 2>&1; then
  echo "xcrun is not installed." >&2
  exit 1
fi

echo "Connected iOS devices:"

found=0
while IFS= read -r line; do
  [[ "$line" == *Simulator* ]] && continue
  if ! printf '%s\n' "$line" | grep -Eq 'iPhone|iPad'; then
    continue
  fi

  udid="$(printf '%s\n' "$line" | grep -Eo '\([0-9A-Fa-f-]{8,}\)' | tail -n1 | tr -d '()')"
  os="$(printf '%s\n' "$line" | grep -Eo '\([0-9]+(\.[0-9]+)*\)' | head -n1 | tr -d '()')"
  name="$(printf '%s\n' "$line" | sed -E 's/ \([0-9]+(\.[0-9]+)*\).*$//')"
  if [[ -n "$udid" ]]; then
    found=1
    printf '  %s\t%s\tiOS %s\n' "$udid" "$name" "${os:-unknown}"
  fi
done < <(xcrun xctrace list devices 2>/dev/null)

if [[ "$found" -eq 0 ]]; then
  echo "  (none found)"
  exit 1
fi
