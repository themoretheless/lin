"""Descriptive peer ratios from the already audited candidate process medians."""
from pathlib import Path
import json
import math
import statistics
import sys

p = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else Path(__file__).resolve().parent
summary = json.loads((p/'summary.json').read_text())
result = {}
for size in ['1k', '10k']:
    lin = summary[f'compare/durable_rollback_1row_{size}/lin']
    sqlite = summary[f'compare/durable_rollback_1row_{size}/sqlite_wal']
    series = {}
    all_pairs = []
    for name, start in [('series1', 1), ('series2', 7)]:
        a = {r['round']: r['candidate_ns'] for r in lin[name]['pairs']}
        b = {r['round']: r['candidate_ns'] for r in sqlite[name]['pairs']}
        assert set(a) == set(b) == set(range(start, start+6))
        pairs = [dict(round=i, lin_ns=a[i], sqlite_ns=b[i], lin_over_sqlite=a[i]/b[i]) for i in sorted(a)]
        assert all(math.isfinite(r['lin_over_sqlite']) and r['lin_over_sqlite'] > 0 for r in pairs)
        series[name] = dict(median_ratio=statistics.median(r['lin_over_sqlite'] for r in pairs),
                            lin_wins=sum(r['lin_over_sqlite'] < 1 for r in pairs), pairs=pairs)
        all_pairs += pairs
    result[size] = dict(series=series, median_ratio=statistics.median(r['lin_over_sqlite'] for r in all_pairs),
                        lin_wins=sum(r['lin_over_sqlite'] < 1 for r in all_pairs), process_pairs=12)
(p/'sqlite-peer-ratios.json').write_text(json.dumps(result, indent=2)+'\n')
lines = ['# Candidate rollback / SQLite WAL', '',
         'Candidate process medians from the complete audited component run. Ratio below 1 means a numeric Lin win. Both APIs use their declared FULL settings; rollback itself performs no WAL synchronization. Native schemas and maintenance differ as documented in the Rust harness. No whole-engine or eight-peer qualification.', '',
         '| Rows | Lin/SQLite median paired ratio | Numeric wins |', '| --- | ---: | ---: |']
for size, r in result.items(): lines.append(f"| {size} | {r['median_ratio']:.6f} | {r['lin_wins']}/12 |")
(p/'sqlite-peer-ratios.md').write_text('\n'.join(lines)+'\n')
print(json.dumps(dict(numeric_wins=sum(r['lin_wins'] for r in result.values()), pairs=24, full_objective_achieved=False)))
