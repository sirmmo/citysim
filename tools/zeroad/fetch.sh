#!/bin/sh
# Shallow, sparse checkout of the 0 A.D. repository at a pinned commit.
#   fetch.sh <dir> <repo> <commit>                 # text data: templates, civs, actors, variants
#   fetch.sh <dir> <repo> <commit> <paths-file>    # plus the listed art files (Git LFS)
set -e
dir=$1; repo=$2; commit=$3; extra=$4
P=binaries/data/mods/public
if [ ! -d "$dir/.git" ]; then
  mkdir -p "$dir"
  git -C "$dir" init -q
  git -C "$dir" remote add origin "$repo"
  git -C "$dir" sparse-checkout init --no-cone
  git -C "$dir" sparse-checkout set "/LICENSE.md" "/$P/simulation/templates/" "/$P/simulation/data/civs/" \
      "/$P/art/actors/" "/$P/art/variants/" "/$P/art/LICENSE.txt"
fi
cd "$dir"
if [ "$(git rev-parse -q --verify HEAD 2>/dev/null)" != "$commit" ]; then
  echo "fetching 0 A.D. $commit (shallow, sparse)" >&2
  GIT_LFS_SKIP_SMUDGE=1 git fetch -q --depth 1 --filter=blob:none origin "$commit"
  GIT_LFS_SKIP_SMUDGE=1 git -c advice.detachedHead=false checkout -q FETCH_HEAD
fi
if [ -n "$extra" ]; then
  # Checking the paths out downloads their blobs and smudges the Git LFS
  # files. (No `git lfs pull`: on a partial clone it lists the whole tree
  # and fetches every blob of the repository.)
  echo "downloading $(wc -l < "$extra") art paths (Git LFS)" >&2
  xargs git sparse-checkout add < "$extra"
fi
