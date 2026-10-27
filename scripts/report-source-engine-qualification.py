#!/usr/bin/env python3
"""Render audited frozen source-engine evidence without inferring all-peer superiority."""
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
experiment_path = p / 'experiment.json'
experiment = json.loads(experiment_path.read_text()) if experiment_path.exists() else {}
layout_path = p / 'layout-policy.json'
layout_note = (json.loads(layout_path.read_text())['description'] if layout_path.exists() else 'Type-layout and retained-memory costs require separate qualification.')
lines = [
    '# ' + experiment.get('title', 'Full-engine source qualification'), '',
    f"Source gate: **{'PASS' if q['source_candidate_qualified'] else 'FAIL'}**. "
    f"{q['processes']} processes, {q['observations']} wall observations. "
    f"Candidate SQLite wins: {q['candidate_sqlite_wins']}/{q['candidate_sqlite_comparisons']}.", '',
    f"Source commit: `{provenance.get('source_commit', 'not recorded')}`. "
    f"Baseline SHA256: `{provenance['binaries']['baseline']['sha256']}`. "
    f"Candidate SHA256: `{provenance['binaries']['candidate']['sha256']}`.", '',
    experiment.get('description', 'Baseline/control share one binary. Candidate uses the declared frozen source changes; see protocol.md for scope and contracts. All-eight-peer superiority remains unproven.'), '',
    'Gain is positive when candidate source is faster than baseline source. '
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
lines += ['', '## SQLite gap in candidate-source processes', '',
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
lines += ['', '## DuckDB read controls and peer gap', '',
          '| Lin case | Series | Lin / DuckDB | Wins /6 |',
          '|---|---|---:|---:|']
duck_wins = duck_total = 0
for case, series in summary.items():
    if not case.endswith('/lin') or case[:-3] + 'duckdb' not in summary:
        continue
    for label, values in series.items():
        peers = {r['round']: r['candidate_ns'] for r in summary[case[:-3] + 'duckdb'][label]['processes']}
        ratios = [r['candidate_ns'] / peers[r['round']] for r in values['processes']]
        wins = sum(ratio < 1 for ratio in ratios)
        duck_wins += wins
        duck_total += len(ratios)
        lines.append(f"| {case} | {label} | {statistics.median(ratios):.4f} | {wins} |")
lines += ['', f'Candidate read DuckDB wins: {duck_wins}/{duck_total}.', '']
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
lines += ['', 'PostgreSQL, MySQL, MSSQL, MongoDB, Kusto and pandas '
          'are outside this run. RSS records cover the full process lifecycle and do not establish '
          'a bound on retained index memory. ' + layout_note + ' No production source is accepted by this report alone.', '']
(p / 'report.md').write_text('\n'.join(lines))
print(p / 'report.md')
