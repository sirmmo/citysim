#!/usr/bin/env bash
# Builds the browser version into dist/web with the pinned Docker image:
#   scripts/web.sh
# Serve the folder with any static web server (it is what the website hosts at
# /citysim/play/), e.g.: python3 -m http.server -d dist/web 8000
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
"$ROOT/scripts/dev.sh" sh -c 'cargo build -q --release --target wasm32-unknown-unknown -p opencity && cargo run -q -p xtask -- web-dist dist/web'
