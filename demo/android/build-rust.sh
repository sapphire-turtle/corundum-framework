#!/bin/bash
# Builds the demo's cdylib for the phones (arm64-v8a) and the emulators (x86_64) and puts it
# in app/src/main/jniLibs, where Gradle packs it into the APK. Needs cargo-ndk and an NDK
# (ANDROID_NDK_HOME). Pass --release for a release build; the ABIs with ABIS="x86_64".
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
workspace="$(cd "$here/../.." && pwd)"

targets=()
for abi in ${ABIS:-arm64-v8a x86_64}; do
  targets+=(-t "$abi")
done

cargo ndk "${targets[@]}" -o "$here/app/src/main/jniLibs" \
  build --manifest-path "$workspace/Cargo.toml" -p corundum-framework-demo --lib "$@"
