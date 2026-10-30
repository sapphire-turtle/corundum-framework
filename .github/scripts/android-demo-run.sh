#!/usr/bin/env bash
# Installs the demo's APK (demo/android) on the running emulator, starts it and checks it is
# still running 20 seconds later. Leaves a screenshot and the logcat in demo/android/ for the
# workflow to upload.
set -euo pipefail
cd "$(dirname "$0")/../../demo/android"

package=net.sapphire_turtle.corundum_demo
adb install -r app/build/outputs/apk/debug/app-debug.apk
adb logcat -c
adb shell am start -W -n "$package/.MainActivity"

sleep 20
adb exec-out screencap -p > screenshot.png
adb logcat -d > logcat.txt || true
echo "---- the demo's log ----"
grep -E "corundum-demo|AndroidRuntime|RustPanic" logcat.txt | tail -n 50 || true
if ! adb shell pidof "$package" > /dev/null; then
  echo "::error::the demo is not running 20 seconds after it started"
  exit 1
fi
if grep -q "corundum demo: run failed" logcat.txt; then
  echo "::error::the demo's window failed"
  exit 1
fi
