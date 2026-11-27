#!/usr/bin/env bash
# Deterministic git bisect helper for Lin correctness or benchmark regressions (LIN-1757).
# Usage:
#   ./scripts/bisect-regression.sh <good-rev> <bad-rev> <cargo-command...>
# Example:
#   ./scripts/bisect-regression.sh v0.7.1 HEAD cargo test --release --test query
set -euo pipefail

if [[ $# -lt 3 ]]; then
  echo "Usage: $0 <good-rev> <bad-rev> <command...>" >&2
  exit 2
fi

GOOD_REV="$1"
BAD_REV="$2"
shift 2

git bisect start "$BAD_REV" "$GOOD_REV"
trap 'git bisect reset >/dev/null 2>&1 || true' EXIT
git bisect run "$@"
