#!/usr/bin/env bash
# Installs the demo app built in demo/ios on the booted simulator $SIM_UDID, launches it and
# checks it is still running 20 seconds later. Leaves a screenshot and the app's console in
# demo/ios/ for the workflow to upload.
set -euo pipefail
cd "$(dirname "$0")/../../demo/ios"

app=build/Build/Products/Debug-iphonesimulator/CorundumDemo.app
bundle_id=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$app/Info.plist")
xcrun simctl install "$SIM_UDID" "$app"

out="$PWD/console.log"
err="$PWD/console-stderr.log"
: > "$out"
: > "$err"
# SpringBoard may not be ready right after boot, so launching can hang or fail: retry with
# a timeout per attempt until simctl prints a pid.
pid=""
for attempt in 1 2 3 4 5 6; do
  echo "launch attempt $attempt"
  result=$(perl -e 'alarm 30; exec @ARGV' xcrun simctl launch --terminate-running-process \
    --stdout="$out" --stderr="$err" "$SIM_UDID" "$bundle_id" 2>&1) || true
  echo "$result"
  pid=$(echo "$result" | sed -n "s/^$bundle_id: *\([0-9][0-9]*\)\$/\1/p")
  [ -n "$pid" ] && break
  sleep 10
done
if [ -z "$pid" ]; then
  echo "::error::the demo never launched"
  exit 1
fi

sleep 20
xcrun simctl io "$SIM_UDID" screenshot screenshot.png
echo "---- console ----"
cat "$out" "$err"
if ! xcrun simctl spawn "$SIM_UDID" launchctl list \
    | grep "UIKitApplication:$bundle_id" | grep -qE '^[0-9]+'; then
  echo "::error::the demo is not running 20 seconds after launch"
  exit 1
fi
if grep -q "run failed" "$out" "$err"; then
  echo "::error::the demo's window failed"
  exit 1
fi
