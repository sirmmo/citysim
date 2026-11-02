#!/usr/bin/env bash
# Bakes 0 A.D. art into the "antiquity" skin (assets/skins/antiquity), in Docker.
#   scripts/bake-0ad.sh [ART_DIR]
# ART_DIR is the folder written by Ovian's tools/zeroad/bake.sh
# (default: /srv/ovian/web/projects/diorama/public/art). The baked files are
# CC BY-SA 3.0 and are not committed (see .gitignore).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ART="${1:-/srv/ovian/web/projects/diorama/public/art}"
[ -f "$ART/manifest.json" ] || { echo "no manifest.json in $ART (run Ovian's tools/zeroad/bake.sh first)" >&2; exit 1; }
OUT="$ROOT/assets/skins/antiquity"
rm -rf "$OUT/models" "$OUT/textures"
docker run --rm -u "$(id -u):$(id -g)" -e HOME=/tmp \
  -v "$ART":/art:ro -v "$ROOT":/w -w /w/tools/zeroad \
  python:3.12.7-slim sh -c \
  "pip install -q --no-cache-dir --root-user-action=ignore --disable-pip-version-check --target /tmp/py pycollada==0.8 numpy==1.26.4 pillow==11.0.0 2>&1 | grep -v 'root user' || true; \
   PYTHONPATH=/tmp/py python bake_0ad.py --art /art --config antiquity.toml --out /w/assets/skins/antiquity"
