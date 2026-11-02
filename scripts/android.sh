#!/usr/bin/env bash
# Builds the Android APK with pinned Docker images:
#   scripts/android.sh
# Output: dist/android/citysim-<version>.apk (debug-signed, for sideloading).
#
# cargo-quad-apk (the APK packager for miniquad/macroquad) embeds Cargo 1.61,
# so the game's crates are first copied to target/android-shim with manifests
# it can parse (xtask android-shim); rustc 1.88.0 still does the compiling.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
"$ROOT/scripts/dev.sh" cargo run -q -p xtask -- android-shim target/android-shim
IMAGE="citysim-android:rust-1.88.0-$(sha256sum "$ROOT/docker/android.Dockerfile" | cut -c1-12)"
if ! docker image inspect "$IMAGE" >/dev/null 2>&1; then
  docker build -t "$IMAGE" -f "$ROOT/docker/android.Dockerfile" "$ROOT/docker"
fi
docker run --rm -i \
  -v "$ROOT":/src \
  -v citysim-android-registry:/usr/local/cargo/registry \
  -w /src/target/android-shim/crates/opencity \
  "$IMAGE" cargo quad-apk build --release "$@"
mkdir -p "$ROOT/dist/android"
version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
apk="$(ls "$ROOT"/target/android-shim/target/android-artifacts/release/apk/*.apk | head -1)"
cp "$apk" "$ROOT/dist/android/citysim-$version.apk"
echo "APK: dist/android/citysim-$version.apk"
