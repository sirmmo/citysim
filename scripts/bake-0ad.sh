#!/usr/bin/env bash
# Downloads 0 A.D. art from its official repository and converts it into the
# ancient culture skins (assets/skins/antiquity-<civ>). Source-checkout helper
# for `opencity-cli bake-0ad`; no Docker, git or Python needed.
#   scripts/bake-0ad.sh [--from <local 0 A.D. checkout>] [--cache <dir>]
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
exec cargo run --quiet --release -p opencity-cli -- --data-dir assets bake-0ad "$@"
