#!/usr/bin/env python3
"""Gate complete warm API measurements against same-run peers and compatible legacy limits.

Exit 2 for a missing comparison, peer loss/tie or compatible-platform >3x legacy
regression; 1 for invalid configuration/data/IO. Cross-platform legacy numbers
remain visible but cannot establish a same-platform performance regression.
"""
from __future__ import annotations
import json
import math
import os
from pathlib import Path
from statistics import median
import sys

WARN_MULT = 1.5
FAIL_MULT = 3.0


def load_medians(data: dict) -> dict[str, float]:
    values: dict[str, list[float]] = {}
    for observation in data.get('observations', []):
        if observation.get('metric') != 'wall':
            continue
        if (observation.get('availability') or {}).get('state') != 'available':
            raise ValueError('unavailable wall observation')
        case = observation.get('case')
        operations = observation.get('operations')
        value = observation.get('value')
        if not isinstance(case, str) or not case:
            raise ValueError('missing wall case ID')
        if isinstance(operations, bool) or not isinstance(operations, int) or operations <= 0:
            raise ValueError('invalid operation count')
        if isinstance(value, bool):
            raise ValueError('invalid wall value')
        value = float(value) / operations
        if not math.isfinite(value) or value <= 0:
            raise ValueError('nonpositive/nonfinite wall value')
        values.setdefault(case, []).append(value)
    return {case: float(median(samples)) for case, samples in values.items()}


def evaluate(baseline: dict, data: dict) -> tuple[list[str], int]:
    if baseline.get('schema') != 2 or not baseline.get('cases'):
        raise ValueError('schema2 baseline with cases required')
    peers = baseline.get('required_peers')
    if (not isinstance(peers, list) or not peers
            or any(not isinstance(p, str) or not p or p == 'lin' for p in peers)
            or len(peers) != len(set(peers))):
        raise ValueError('explicit distinct peer engines required')
    platform = baseline.get('platform') or {}
    environment = data.get('environment') or {}
    if any(not isinstance(d.get(k), str) or not d[k] for d in [platform, environment] for k in ['os', 'arch']):
        raise ValueError('OS/architecture metadata required')
    if data.get('status') != 'complete':
        raise ValueError('benchmark run incomplete')
    comparable = all(platform[k] == environment[k] for k in ['os', 'arch'])
    medians = load_medians(data)
    failures = 0
    lines = ['### Bench budget (same-run peers and legacy platform limits)', '',
             f"Run: {environment['os']}/{environment['arch']}; legacy: {platform['os']}/{platform['arch']}.",
             'Same OS/architecture does not imply identical hardware.', '']
    for case, specification in baseline['cases'].items():
        if not case.endswith('/lin'):
            raise ValueError('gated case must identify Lin')
        historical = float(specification['median_ns_per_op'])
        if not math.isfinite(historical) or historical <= 0:
            raise ValueError('invalid historical median')
        current = medians.get(case)
        if current is None:
            lines.append(f'- `{case}`: FAIL missing Lin measurement')
            failures += 1
            continue
        ratio = current / historical
        status = 'historical only: OS/architecture mismatch'
        if comparable:
            status = f'FAIL {ratio:.2f}x' if ratio > FAIL_MULT else f'WARN {ratio:.2f}x' if ratio > WARN_MULT else f'ok {ratio:.2f}x'
            failures += ratio > FAIL_MULT
        lines.append(f'- `{case}`: {current:.1f} ns/op; legacy {historical:.1f}, ratio {ratio:.2f}x; {status}')
        prefix = case.rsplit('/', 1)[0]
        for peer in peers:
            reference = medians.get(prefix + '/' + peer)
            if reference is None:
                lines.append(f'  - {peer}: FAIL missing same-run peer')
                failures += 1
            elif current >= reference:
                lines.append(f'  - {peer}: FAIL Lin {current:.1f} >= peer {reference:.1f} ns/op')
                failures += 1
            else:
                lines.append(f'  - {peer}: win {reference/current:.2f}x ({reference:.1f} ns/op)')
    return lines, int(failures)


def main() -> int:
    root = Path(__file__).resolve().parents[1]
    baseline_path = Path(os.environ.get('BENCH_BASELINE', root / 'benches/ci-baseline.json'))
    run_path = Path(os.environ.get('BENCH_RUN_JSON', root / '.airbug-bench/ci/run.json'))
    try:
        lines, failures = evaluate(json.loads(baseline_path.read_text()), json.loads(run_path.read_text()))
    except (OSError, ValueError, TypeError, KeyError) as error:
        print(f'bench budget invalid: {error}', file=sys.stderr)
        return 1
    text = '\n'.join(lines)
    print(text)
    if summary := os.environ.get('GITHUB_STEP_SUMMARY'):
        with open(summary, 'a', encoding='utf-8') as output:
            output.write('\n' + text + '\n')
    return 2 if failures else 0


if __name__ == '__main__':
    sys.exit(main())
