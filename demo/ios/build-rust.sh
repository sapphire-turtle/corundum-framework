#!/bin/bash
# Xcode's pre-build script: builds the demo's staticlib for the SDK Xcode is targeting and
# copies it to $BUILT_PRODUCTS_DIR/rust/, where the app links it.
set -euo pipefail

# Resolve everything from this script's location, not the caller's cwd.
here="$(cd "$(dirname "$0")" && pwd)"
workspace="$(cd "$here/../.." && pwd)"
# An explicit target dir overrides any CARGO_TARGET_DIR in Xcode's environment, so the
# output path below is known.
target_dir="$workspace/target"

# Xcode's script environment does not include rustup's shims.
export PATH="$HOME/.cargo/bin:$PATH"

# The staticlib is arm64 only: no x86_64 simulator slice is built.
for arch in ${ARCHS:-arm64}; do
  if [ "$arch" != "arm64" ]; then
    echo "error: ARCHS contains '$arch'; only arm64 is supported (an Apple-silicon Mac for the simulator)" >&2
    exit 1
  fi
done

case "${PLATFORM_NAME:?PLATFORM_NAME is not set}" in
  iphoneos) target=aarch64-apple-ios ;;
  iphonesimulator) target=aarch64-apple-ios-sim ;;
  *) echo "error: unsupported PLATFORM_NAME '$PLATFORM_NAME'" >&2; exit 1 ;;
esac

profile_dir=debug
release_flag=()
if [ "${CONFIGURATION:-}" = "Release" ]; then
  profile_dir=release
  release_flag=(--release)
fi

lib="$target_dir/$target/$profile_dir/libcorundum_demo.a"
echo "build-rust: target=$target profile=$profile_dir"

cargo build --manifest-path "$workspace/Cargo.toml" -p corundum-framework-demo --lib \
  --target "$target" --target-dir "$target_dir" \
  ${release_flag[@]+"${release_flag[@]}"}

if [ ! -f "$lib" ]; then
  echo "error: $lib not found" >&2
  exit 1
fi

mkdir -p "$BUILT_PRODUCTS_DIR/rust"
cp "$lib" "$BUILT_PRODUCTS_DIR/rust/"
