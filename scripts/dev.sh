#!/usr/bin/env bash
# Run any command inside the pinned dev container, e.g.:
#   scripts/dev.sh cargo test --workspace
#   scripts/dev.sh cargo clippy --workspace --all-targets
#   scripts/dev.sh xvfb-run -a -s "-screen 0 1280x800x24" /target/debug/opencity --screenshot out.png
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# The tag follows the Dockerfile content, so editing it triggers a rebuild.
IMAGE="opencity-rs-dev:rust-1.88.0-$(sha256sum "$ROOT/docker/Dockerfile" | cut -c1-12)"
if ! docker image inspect "$IMAGE" >/dev/null 2>&1; then
  docker build -t "$IMAGE" -f "$ROOT/docker/Dockerfile" "$ROOT/docker"
fi
exec docker run --rm -i \
  -v "$ROOT":/src \
  -v opencity-rs-cargo-registry:/usr/local/cargo/registry \
  -v opencity-rs-target:/target \
  -e CARGO_TERM_COLOR="${CARGO_TERM_COLOR:-$([ -t 1 ] && echo always || echo never)}" \
  "$IMAGE" "$@"
