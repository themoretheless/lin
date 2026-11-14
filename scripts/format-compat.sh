#!/usr/bin/env bash
# Cross-version check of the on-disk format: the current tree and a previous release must
# read each other's cold-tier databases and give the same answers.
#
#   scripts/format-compat.sh [<previous-ref>]
#
# <previous-ref> defaults to $LIN_COMPAT_REF, then to the newest `v*` tag that is not HEAD.
# The previous release is built in a temporary git worktree with the current
# examples/format_compat.rs copied in, so that file may only use API both versions have.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

prev_ref="${1:-${LIN_COMPAT_REF:-}}"
if [ -z "$prev_ref" ]; then
  head_sha="$(git rev-parse HEAD)"
  for tag in $(git tag --list 'v*' --sort=-version:refname); do
    if [ "$(git rev-parse "$tag^{commit}")" != "$head_sha" ]; then
      prev_ref="$tag"
      break
    fi
  done
fi
if [ -z "$prev_ref" ]; then
  echo "format-compat: no previous release ref found (pass one or set LIN_COMPAT_REF)" >&2
  exit 2
fi
prev_sha="$(git rev-parse "$prev_ref^{commit}")"
echo "format-compat: current $(git rev-parse --short HEAD) (working tree) vs previous $prev_ref ($(git rev-parse --short "$prev_sha"))"

work="$(mktemp -d "${TMPDIR:-/tmp}/lin-format-compat.XXXXXX")"
prev_tree="$work/prev"
cleanup() {
  git worktree remove --force "$prev_tree" >/dev/null 2>&1 || true
  rm -rf "$work"
}
trap cleanup EXIT

git worktree add --detach "$prev_tree" "$prev_sha" >/dev/null
mkdir -p "$prev_tree/examples"
cp examples/format_compat.rs "$prev_tree/examples/format_compat.rs"

cargo build --release --locked --example format_compat --bin lin
(cd "$prev_tree" && CARGO_TARGET_DIR="$work/prev-target" cargo build --release --locked --example format_compat)

cur="$root/${CARGO_TARGET_DIR:-target}/release/examples/format_compat"
cur_cli="$root/${CARGO_TARGET_DIR:-target}/release/lin"
prev="$work/prev-target/release/examples/format_compat"

fail=0
compare() {
  if ! diff -u "$2" "$3" > "$work/diff.txt"; then
    echo "FAIL: $1" >&2
    head -40 "$work/diff.txt" >&2
    fail=1
  else
    echo "ok:   $1"
  fi
}

# 1. Written by the current tree (segments + overlay DML left in the WAL).
"$cur" write "$work/db-new"
"$cur" read "$work/db-new" > "$work/new.by-new.txt"
"$prev" read "$work/db-new" > "$work/new.by-prev.txt"
compare "previous release reads a database written by the current tree" \
  "$work/new.by-new.txt" "$work/new.by-prev.txt"
"$cur" read "$work/db-new" > "$work/new.by-new-again.txt"
compare "current tree reads it back after the previous release opened it" \
  "$work/new.by-new.txt" "$work/new.by-new-again.txt"

# 2. Same, after the current-only features: cluster key and clustered compaction.
"$cur" write "$work/db-clustered"
"$cur_cli" --data "$work/db-clustered" --cold cluster items n >/dev/null
"$cur_cli" --data "$work/db-clustered" --cold compact items >/dev/null
"$cur" read "$work/db-clustered" > "$work/clustered.by-new.txt"
"$prev" read "$work/db-clustered" > "$work/clustered.by-prev.txt"
compare "clustered and compacted database gives the same answers as the plain one" \
  "$work/new.by-new.txt" "$work/clustered.by-new.txt"
compare "previous release reads the clustered and compacted database" \
  "$work/clustered.by-new.txt" "$work/clustered.by-prev.txt"

# 3. Written by the previous release.
"$prev" write "$work/db-prev"
"$prev" read "$work/db-prev" > "$work/prev.by-prev.txt"
"$cur" read "$work/db-prev" > "$work/prev.by-new.txt"
compare "current tree reads a database written by the previous release" \
  "$work/prev.by-prev.txt" "$work/prev.by-new.txt"
compare "both versions build the same database from the same statements" \
  "$work/new.by-new.txt" "$work/prev.by-prev.txt"

if [ "$fail" -ne 0 ]; then
  echo "format-compat: FAILED" >&2
  exit 1
fi
echo "format-compat: all checks passed"
