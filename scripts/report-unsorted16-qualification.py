#!/usr/bin/env python3
"""Render audited runtime evidence; never infer source or all-peer qualification."""
import argparse
import json
from pathlib import Path
import statistics
import subprocess
import sys

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('results', type=Path)
args = parser.parse_args()
p = args.results.resolve()
subprocess.run([sys.executable, str(p / 'verify.py')], check=True)
summary = json.loads((p / 'summary.json').read_text())
q = json.loads((p / 'qualification.json').read_text())
provenance = json.loads((p / 'provenance.json').read_text())
lines = [
    '# Uninstrumented allocator runtime qualification', '',
    f"Runtime gate: **{'PASS' if q['runtime_candidate_qualified'] else 'FAIL'}**. "
    f"{q['processes']} processes, {q['observations']} wall observations. "
    f"Candidate SQLite wins: {q['candidate_sqlite_wins']}/{q['candidate_sqlite_comparisons']}.", '',
    f"Source commit: `{provenance.get('source_commit', 'not recorded')}`. "
    f"Binary SHA256: `{provenance.get('binary_sha256', 'not recorded')}`.", '',
    'One identical Map/static-output prototype binary runs in all roles. '
    'The candidate changes the process glibc runtime setting for both engines. '
    'This measures runtime impact; it does not qualify the prototype against production main. '
    'All-eight-peer superiority is unproven.', '',
    'Gain is positive when candidate runtime is faster than inherited runtime. '
    'A/A is the median absolute baseline/control difference. '
    'Each series contains six process pairs; every control remains in the table.', '',
    '| Case | Series | Gain % | A/A % | Positive /6 | Gate |',
    '|---|---|---:|---:|---:|---|',
]
for case, series in summary.items():
    for label, s in series.items():
        lines.append(f"| {case} | {label} | {s['gain_percent']:+.3f} | "
                     f"{s['aa_absolute_percent']:.3f} | {s['positive_pairs']} | "
                     f"{'PASS' if s['passes'] else 'FAIL'} |")
lines += ['', '## SQLite gap under the same candidate runtime', '',
          'Ratios are medians of per-round Lin/SQLite latency ratios, not ratios of pooled samples. '
          'A ratio below 1 means Lin is faster. Numeric wins alone do not establish statistical superiority.', '',
          '| Lin case | Series | Lin / SQLite | Wins /6 |',
          '|---|---|---:|---:|']
for case, series in summary.items():
    if not case.endswith('/lin'):
        continue
    for label, s in series.items():
        ratio = statistics.median(r['candidate_ns'] / r['candidate_sqlite_ns'] for r in s['processes'])
        lines.append(f"| {case} | {label} | {ratio:.4f} | {s['sqlite_wins']} |")
lines += ['', '## Whole-process peak RSS', '',
          'KiB from GNU time for the complete fixture lifecycle. '
          'The delta compares medians; the maximum preserves isolated high-water observations. '
          'These are process peaks, not retained-cache measurements or per-operation allocation counts.', '',
          '| Case | Series | Baseline median KiB | Control median KiB | Candidate median KiB | Candidate delta % | Candidate max KiB |',
          '|---|---|---:|---:|---:|---:|---:|']
records = json.loads((p / 'process-status.json').read_text())
for case, series in summary.items():
    for label, s in series.items():
        rss = s['peak_rss_kib']
        rounds = range(1, 7) if label == 'series1' else range(7, 13)
        maximum = max(r['peak_rss_kib'] for r in records
                      if r['case'] == case and r['mode'] == 'candidate' and r['round'] in rounds)
        delta = 100 * (rss['candidate'] / rss['baseline'] - 1)
        lines.append(f"| {case} | {label} | {rss['baseline']:.0f} | {rss['control']:.0f} | "
                     f"{rss['candidate']:.0f} | {delta:+.3f} | {maximum} |")
lines += ['', 'Read workloads and DuckDB, PostgreSQL, MySQL, MSSQL, MongoDB, Kusto and pandas '
          'are outside this run. RSS records cover the full process lifecycle and do not establish '
          'a bound on retained allocator caches. No production setting is accepted by this report alone.', '']
(p / 'report.md').write_text('\n'.join(lines))
print(p / 'report.md')
