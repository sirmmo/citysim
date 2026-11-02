#!/usr/bin/env bash
# Downloads the 0 A.D. buildings and trees for the ancient cultures and converts
# them (about 115 MB, from the official 0 A.D. repository). No Docker needed.
cd "$(dirname "$0")" && exec ./opencity-cli bake-0ad "$@"
