#!/usr/bin/env python3
"""Compare airbug run.json Lin medians to benches/ci-baseline.json.

Warn if any gated case is >1.5× slower than baseline.
Fail (exit 2) if any gated case is >3× slower.
Exit 0 on warn-only or all ok. Exit 1 on parse/IO errors.
"""
from __future__ import annotations

import json
import os
import sys
from collections import defaultdict
from pathlib import Path
from statistics import median

WARN_MULT = 1.5
FAIL_MULT = 3.0


def load_medians(run_path: Path) -> dict[str, float]:
    data = json.loads(run_path.read_text())
    vals: dict[str, list[float]] = defaultdict(list)
    for o in data.get("observations", []):
        if o.get("metric") != "wall":
            continue
        avail = o.get("availability") or {}
        if avail.get("state") != "available":
            continue
        case = o.get("case")
        if not case:
            continue
        ops = int(o.get("operations") or 1) or 1
        vals[case].append(float(o["value"]) / ops)
    return {k: float(median(v)) for k, v in vals.items() if v}


def main() -> int:
    root = Path(__file__).resolve().parents[1]
    baseline_path = Path(os.environ.get("BENCH_BASELINE", root / "benches/ci-baseline.json"))
    run_path = Path(os.environ.get("BENCH_RUN_JSON", root / ".airbug-bench/ci/run.json"))
    if not baseline_path.is_file():
        print(f"baseline missing: {baseline_path}", file=sys.stderr)
        return 1
    if not run_path.is_file():
        print(f"run.json missing: {run_path}", file=sys.stderr)
        return 1

    baseline = json.loads(baseline_path.read_text())
    cases = baseline.get("cases") or {}
    medians = load_medians(run_path)

    lines = ["### Bench budget (Lin vs baseline)", ""]
    warn_n = 0
    fail_n = 0
    for case, spec in cases.items():
        base = float(spec["median_ns_per_op"])
        got = medians.get(case)
        if got is None:
            lines.append(f"- `{case}`: **missing** in run.json")
            warn_n += 1
            continue
        ratio = got / base if base > 0 else float("inf")
        status = "ok"
        if ratio > FAIL_MULT:
            status = f"FAIL {ratio:.2f}×"
            fail_n += 1
        elif ratio > WARN_MULT:
            status = f"WARN {ratio:.2f}×"
            warn_n += 1
        else:
            status = f"ok {ratio:.2f}×"
        lines.append(
            f"- `{case}`: {got:.1f} ns/op vs baseline {base:.1f} → {status}"
        )

    text = "\n".join(lines)
    print(text)
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a", encoding="utf-8") as f:
            f.write("\n" + text + "\n")

    if fail_n:
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
