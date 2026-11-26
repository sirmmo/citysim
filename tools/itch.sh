#!/bin/sh
# Uploads citysim to itch.io (https://itch.io) with butler, each game as an HTML5 build:
#   sh tools/itch.sh [game]     citysim (the only one)
# It uploads exactly what goes to Wavedash: tools/wavedash.sh, run with WAVEDASH_DRY_RUN=1, fills
# each game's build folder and uploads nothing; then each folder goes to its itch.io page as
# ITCH_USER/<page>:html5 (ITCH_USER defaults to sirmmo; the pages are in PAGES below, or
# ITCH_GAME_<GAME>, e.g. ITCH_GAME_CITYSIM). Each page must exist on itch.io first (Dashboard >
# Create new project, kind HTML). butler takes BUTLER_API_KEY from the environment, or its own
# `butler login`. Without butler on PATH, or with ITCH_DRY_RUN=1, it only says what it would push.
set -e
cd "$(dirname "$0")/.."
ITCH_USER=${ITCH_USER:-sirmmo}
# game (as tools/wavedash.sh names it) = its itch.io page
PAGES="citysim=citysim"

page() {
  eval "p=\${ITCH_GAME_$(echo "$1" | tr 'a-z-' 'A-Z_'):-}"
  [ -n "$p" ] || for e in $PAGES; do [ "${e%%=*}" = "$1" ] && p=${e#*=}; done
  echo "${p:-$1}"
}

# The builds, as for Wavedash; its lines "<game>: would push <folder> ..." say where each one is.
log=$(mktemp)
(WAVEDASH_DRY_RUN=1 sh tools/wavedash.sh "$@"; echo $? > "$log.status") | tee "$log" | grep -v ': would push ' || true
[ "$(cat "$log.status")" = 0 ] || exit 1
version=${ITCH_VERSION:-$(git rev-parse --short=12 HEAD 2>/dev/null || echo local)}
sed -n 's/^\([^:]*\): would push \([^ ]*\) .*/\1 \2/p' "$log" | while read -r game dir; do
  target="$ITCH_USER/$(page "$game"):html5"
  if command -v butler >/dev/null 2>&1 && [ -z "${ITCH_DRY_RUN:-}" ]; then
    echo "$game: pushing $dir to $target"
    butler push "$dir" "$target" --userversion "$version"
  else
    echo "$game: would push $dir to $target (butler push $dir $target --userversion $version)"
  fi
done
rm -f "$log" "$log.status"
