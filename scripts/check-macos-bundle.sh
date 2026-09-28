#!/bin/bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="${1:-$ROOT/src-tauri/target/release/bundle/macos/DECC.app}"
PLIST="$APP/Contents/Info.plist"

if [[ ! -d "$APP" || ! -f "$PLIST" ]]; then
  echo "macOS bundle is missing: $APP" >&2
  exit 1
fi

read_json() {
  /usr/bin/env node -e '
    const fs = require("node:fs");
    const [file, key] = process.argv.slice(1);
    const value = key.split(".").reduce((current, part) => current?.[part], JSON.parse(fs.readFileSync(file, "utf8")));
    if (value === undefined || value === null) process.exit(2);
    process.stdout.write(String(value));
  ' "$1" "$2"
}

EXPECTED_IDENTIFIER="$(read_json "$ROOT/src-tauri/tauri.conf.json" identifier)"
EXPECTED_NAME="$(read_json "$ROOT/src-tauri/tauri.conf.json" productName)"
EXPECTED_VERSION="$(read_json "$ROOT/src-tauri/tauri.conf.json" version)"

plist_value() {
  /usr/libexec/PlistBuddy -c "Print :$1" "$PLIST"
}

ACTUAL_IDENTIFIER="$(plist_value CFBundleIdentifier)"
ACTUAL_NAME="$(plist_value CFBundleName)"
ACTUAL_DISPLAY_NAME="$(plist_value CFBundleDisplayName)"
ACTUAL_VERSION="$(plist_value CFBundleShortVersionString)"
ACTUAL_BUILD_VERSION="$(plist_value CFBundleVersion)"
ACTUAL_EXECUTABLE="$(plist_value CFBundleExecutable)"

[[ "$ACTUAL_IDENTIFIER" == "$EXPECTED_IDENTIFIER" ]] || {
  echo "Bundle identifier mismatch: expected $EXPECTED_IDENTIFIER, got $ACTUAL_IDENTIFIER" >&2
  exit 1
}
[[ "$ACTUAL_NAME" == "$EXPECTED_NAME" ]] || {
  echo "Bundle name mismatch: expected $EXPECTED_NAME, got $ACTUAL_NAME" >&2
  exit 1
}
[[ "$ACTUAL_DISPLAY_NAME" == "$EXPECTED_NAME" ]] || {
  echo "Bundle display name mismatch: expected $EXPECTED_NAME, got $ACTUAL_DISPLAY_NAME" >&2
  exit 1
}
[[ "$ACTUAL_VERSION" == "$EXPECTED_VERSION" ]] || {
  echo "Bundle version mismatch: expected $EXPECTED_VERSION, got $ACTUAL_VERSION" >&2
  exit 1
}
[[ "$ACTUAL_BUILD_VERSION" == "$EXPECTED_VERSION" ]] || {
  echo "Bundle build version mismatch: expected $EXPECTED_VERSION, got $ACTUAL_BUILD_VERSION" >&2
  exit 1
}
[[ -x "$APP/Contents/MacOS/$ACTUAL_EXECUTABLE" ]] || {
  echo "Bundle executable is missing or not executable: $ACTUAL_EXECUTABLE" >&2
  exit 1
}

DMG_DIR="$ROOT/src-tauri/target/release/bundle/dmg"
DMG_COUNT="$(/usr/bin/find "$DMG_DIR" -maxdepth 1 -type f -name "${EXPECTED_NAME}_${EXPECTED_VERSION}_*.dmg" 2>/dev/null | /usr/bin/wc -l | /usr/bin/tr -d ' ')"
[[ "$DMG_COUNT" == "1" ]] || {
  echo "Expected exactly one DMG for $EXPECTED_NAME $EXPECTED_VERSION, found $DMG_COUNT." >&2
  exit 1
}
DMG="$(/usr/bin/find "$DMG_DIR" -maxdepth 1 -type f -name "${EXPECTED_NAME}_${EXPECTED_VERSION}_*.dmg" -print | /usr/bin/head -n 1)"
/usr/bin/hdiutil verify "$DMG" >/dev/null || {
  echo "DMG verification failed: $DMG" >&2
  exit 1
}
DMG_SHA256="$(/usr/bin/shasum -a 256 "$DMG" | /usr/bin/awk '{print $1}')"
DMG_SIZE_BYTES="$(/usr/bin/stat -f%z "$DMG")"

SIGNING_OUTPUT="$(/usr/bin/codesign -dv --verbose=4 "$APP" 2>&1 || true)"
if /usr/bin/grep -q "Developer ID Application" <<<"$SIGNING_OUTPUT"; then
  SIGNING_MODE="developer-id"
elif /usr/bin/grep -q "adhoc" <<<"$SIGNING_OUTPUT"; then
  SIGNING_MODE="adhoc"
else
  SIGNING_MODE="unsigned-or-unknown"
fi

if /usr/sbin/spctl --assess --type execute -vv "$APP" >/tmp/decc-spctl.out 2>&1; then
  GATEKEEPER="accepted"
else
  GATEKEEPER="rejected"
fi

SIZE_KB="$(/usr/bin/du -sk "$APP" | /usr/bin/awk '{print $1}')"

echo "macOS bundle OK · $ACTUAL_NAME $ACTUAL_VERSION · $ACTUAL_IDENTIFIER · ${SIZE_KB}KB"
echo "DMG: $DMG · ${DMG_SIZE_BYTES} bytes · SHA256 $DMG_SHA256"
echo "Signing mode: $SIGNING_MODE"
echo "Gatekeeper: $GATEKEEPER"

if [[ "${DECC_REQUIRE_DISTRIBUTION_SIGNING:-0}" == "1" ]]; then
  [[ "$SIGNING_MODE" == "developer-id" ]] || {
    echo "Distribution verification requires a Developer ID Application signature." >&2
    exit 1
  }
  [[ "$GATEKEEPER" == "accepted" ]] || {
    echo "Distribution verification requires Gatekeeper acceptance/notarization." >&2
    /bin/cat /tmp/decc-spctl.out >&2 || true
    exit 1
  }
fi
