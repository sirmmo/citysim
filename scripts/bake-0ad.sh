#!/usr/bin/env bash
# Bakes 0 A.D. art from the original repository into one skin per culture
# (assets/skins/antiquity-<civ>), in Docker. The game runs this on first launch.
#   scripts/bake-0ad.sh
# The checkout (only the files the mapping needs) is cached in the Docker volume
# citysim-0ad. The baked files are CC BY-SA 3.0 and are not committed.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
IMAGE="citysim-zeroad:$(sha256sum "$ROOT/docker/zeroad.Dockerfile" | cut -c1-12)"
if ! docker image inspect "$IMAGE" >/dev/null 2>&1; then
  docker build -q -t "$IMAGE" -f "$ROOT/docker/zeroad.Dockerfile" "$ROOT/docker" >/dev/null
fi
rm -rf "$ROOT"/assets/skins/antiquity-* "$ROOT/assets/skins/generated.toml"
docker run --rm -v citysim-0ad:/cache -v "$ROOT":/w -w /w "$IMAGE" sh -c \
  "python tools/zeroad/bake_0ad.py --cache /cache/repo --config tools/zeroad/antiquity.toml --out assets/skins \
   && chown -R $(id -u):$(id -g) assets/skins"
