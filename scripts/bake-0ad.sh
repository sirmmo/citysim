#!/usr/bin/env bash
# Downloads 0 A.D. art from the original repository and converts it into one
# skin per culture (assets/skins/antiquity-<civ>), in Docker.
#   scripts/bake-0ad.sh          (or ./download-0ad-art.sh in a release package)
# Needs Docker. The checkout (only the files the mapping needs) is cached in the
# Docker volume citysim-0ad. The converted files are CC BY-SA 3.0.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if ! command -v docker >/dev/null 2>&1; then
  echo "Docker is needed to download the 0 A.D. art: https://docs.docker.com/get-docker/" >&2
  exit 1
fi
# Image tag follows the Dockerfile content (cksum is available on Linux and macOS).
IMAGE="citysim-zeroad:$(cksum < "$ROOT/docker/zeroad.Dockerfile" | cut -d' ' -f1)"
if ! docker image inspect "$IMAGE" >/dev/null 2>&1; then
  echo "Preparing the conversion tools (first time only)..." >&2
  docker build -q -t "$IMAGE" -f "$ROOT/docker/zeroad.Dockerfile" "$ROOT/docker" >/dev/null
fi
rm -rf "$ROOT"/assets/skins/antiquity-* "$ROOT/assets/skins/generated.toml"
docker run --rm -v citysim-0ad:/cache -v "$ROOT":/w -w /w "$IMAGE" sh -c \
  "python tools/zeroad/bake_0ad.py --cache /cache/repo --config tools/zeroad/antiquity.toml --out assets/skins \
   && chown -R $(id -u):$(id -g) assets/skins"
echo "0 A.D. art installed: start citysim and pick an ancient culture in File > New city."
