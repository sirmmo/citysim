#!/usr/bin/env bash
# Turns a citysim release into a game that only plays one pack.
#
#   scripts/make-pack.sh <pack-id> <release archive or unpacked folder> [out-dir]
#
# The pack comes from assets/packs/<pack-id>.toml (in this repository, or in
# the release when the repository has no such pack). The result is
# <out-dir>/<pack-id>-<release name>/, plus a .zip or .tar.gz like the
# release: the same game with assets/pack.toml, so it opens straight into
# the pack, with the skin and rules locked. Nothing is downloaded or built.
set -euo pipefail

id=${1:?usage: make-pack.sh <pack-id> <release archive or folder> [out-dir]}
src=${2:?usage: make-pack.sh <pack-id> <release archive or folder> [out-dir]}
out=${3:-dist/packs}
here=$(cd "$(dirname "$0")/.." && pwd)

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
case "$src" in
  *.tar.gz) tar xzf "$src" -C "$work"; src=$(find "$work" -mindepth 1 -maxdepth 1 -type d | head -1) ;;
  *.zip) unzip -q "$src" -d "$work"; src=$(find "$work" -mindepth 1 -maxdepth 1 -type d | head -1) ;;
esac
[[ -d "$src/assets" ]] || { echo "no assets/ in $src: not a citysim release" >&2; exit 1; }

pack="$here/assets/packs/$id.toml"
[[ -f "$pack" ]] || pack="$src/assets/packs/$id.toml"
[[ -f "$pack" ]] || { echo "no pack $id (assets/packs/$id.toml)" >&2; exit 1; }
grep -q '^scenario *=' "$pack" || { echo "$pack names no scenario" >&2; exit 1; }

name="$id-$(basename "$src")"
dest="$out/$name"
rm -rf "$dest"
mkdir -p "$out"
cp -r "$src" "$dest"
mkdir -p "$dest/assets/packs"
cp "$pack" "$dest/assets/packs/$id.toml"
{ echo "id = \"$id\""; grep -v '^id *=' "$pack"; } > "$dest/assets/pack.toml"
title=$(sed -n 's/^name *= *"\(.*\)"/\1/p' "$pack" | head -1)
cat > "$dest/PACK.txt" <<TXT
$title

This is citysim sealed to one game. Run opencity (opencity.exe on Windows)
from this folder: it opens straight into "$title", with its look and rules
fixed. Your game is saved when you quit, and comes back at the next launch.
Delete assets/pack.toml to get the full citysim back.
TXT

cd "$out"
if [[ "$name" == *windows* ]]; then
  rm -f "$name.zip"; (command -v zip >/dev/null && zip -qr "$name.zip" "$name") || python3 -m zipfile -c "$name.zip" "$name"
  echo "$out/$name.zip"
else
  tar czf "$name.tar.gz" "$name"
  echo "$out/$name.tar.gz"
fi
