#!/usr/bin/env bash
# Local parity with CI benches + optional airbug-hub GUID registration.
# Env: AIRBUG_HUB=http://127.0.0.1:8790 (default) — set empty to skip register.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

HUB="${AIRBUG_HUB-http://127.0.0.1:8790}"
REPORT="${BENCH_REPORT:-bench-report.md}"
DASH_URL="${AIRBUG_DASH_URL:-}"

OUT=""
RUN_ID=""
HUB_ID=""

if [[ -n "$HUB" ]]; then
  if ! curl -sf --max-time 1 "${HUB%/}/api/v1/status" >/dev/null; then
    echo "airbug hub not reachable at $HUB"
    echo "  cargo run -p airbug-hub --manifest-path ../airbug/dash/hub/Cargo.toml -- serve --root \"$ROOT\""
    exit 1
  fi
  REG=$(curl -sf --max-time 5 -X POST "${HUB%/}/api/v1/bench/runs" \
    -H 'content-type: application/json' \
    -d '{"title":"lin compare","command":"scripts/ci-bench.sh"}')
  OUT=$(python3 -c 'import json,sys; print(json.load(sys.stdin)["out_dir"])' <<<"$REG")
  RUN_ID=$(python3 -c 'import json,sys; print(json.load(sys.stdin)["run_id"])' <<<"$REG")
  HUB_ID=$(python3 -c 'import json,sys; print(json.load(sys.stdin)["hub_id"])' <<<"$REG")
  DASH_URL=$(python3 -c 'import json,sys; print(json.load(sys.stdin)["dash_url"])' <<<"$REG")
  export OTEL_RESOURCE_ATTRIBUTES="airbug.hub_id=${HUB_ID},airbug.run_id=${RUN_ID}${OTEL_RESOURCE_ATTRIBUTES:+,$OTEL_RESOURCE_ATTRIBUTES}"
  echo "airbug dash: $DASH_URL"
  if [[ "${AIRBUG_DASH_OPEN:-1}" == "1" ]]; then
    if command -v open >/dev/null 2>&1; then open "$DASH_URL"
    elif command -v xdg-open >/dev/null 2>&1; then xdg-open "$DASH_URL"; fi
  fi
else
  OUT="${BENCH_OUT:-.airbug-bench/ci}"
  rm -rf "$OUT"
  mkdir -p "$(dirname "$OUT")"
  echo "airbug dash: (AIRBUG_HUB unset — local output only)"
fi

cargo bench --bench compare -- \
  --profile quick \
  --exclude 'compare/durable*' \
  --exclude 'compare/cold*' \
  --exclude 'compare/wal*' \
  --exclude 'compare/hot_reopen*' \
  --output "$OUT" \
  | tee "$REPORT"

cp "$REPORT" "$OUT/bench-report.md"
echo "Wrote $REPORT and $OUT/ (run.json + report.html)"
if [[ -n "$DASH_URL" ]]; then
  echo "airbug dash: $DASH_URL"
fi
