from pathlib import Path
import json
import math
import statistics as st
import sys
sys.path.insert(0, str(Path.cwd()/'scripts'))
from benchmark_role_balance import schedule, orders_from_records

p = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else Path(__file__).resolve().parent
cases = json.loads((p/'cases.json').read_text())
roles = ['baseline_a', 'candidate_a', 'baseline_b', 'candidate_b', 'baseline_c', 'candidate_c']
records = json.loads((p/'measurement/process-status.json').read_text())
plan = schedule(cases, roles)
assert [(r['round'], r['case'], r['role']) for r in records] == [(r['round'], r['workload'], r['role']) for r in plan]
orders_from_records(records, [c['id'] for c in cases], roles, 'case')
med = {}
ids = set()
for r in records:
    assert r['exit_code'] == 0
    d = json.loads((p/'measurement'/r['output']/'run.json').read_text())
    assert d['status'] == 'complete' and d['id'] not in ids
    ids.add(d['id'])
    assert len(d['cases']) == 1 and d['cases'][0]['id'] == r['case'] and len(d['observations']) == 8
    values = []
    for o in d['observations']:
        v = float(o['value'])
        assert o['case'] == r['case'] and o['metric'] == 'wall' and o['operations'] == 1
        assert o['availability']['state'] == 'available' and math.isfinite(v) and v > 0
        values.append(v)
    med[r['round'], r['case'], r['role']] = st.median(values)
summary = {}
for c in cases:
    result = {}
    for series, start in [('series1', 1), ('series2', 7)]:
        pairs = []
        for rnd in range(start, start+6):
            values = {role: med[rnd, c['id'], role] for role in roles}
            a = st.median(values['baseline_'+suffix] for suffix in 'abc')
            b = st.median(values['candidate_'+suffix] for suffix in 'abc')
            nulls = {source+'_'+left+right: 100*(1-values[source+'_'+right]/values[source+'_'+left])
                     for source in ['baseline', 'candidate'] for left, right in [('a', 'b'), ('a', 'c'), ('b', 'c')]}
            pairs.append(dict(round=rnd, role_ns=values, baseline_group_ns=a, candidate_group_ns=b,
                              cross_build_gain_percent=100*(1-b/a), null_gains_percent=nulls))
        gains = [row['cross_build_gain_percent'] for row in pairs]
        result[series] = dict(pairs=pairs, cross_build_gain_percent=st.median(gains),
                              positive_rounds=sum(v > 0 for v in gains),
                              nulls={key: dict(gain_percent=st.median(row['null_gains_percent'][key] for row in pairs),
                                               positive_rounds=sum(row['null_gains_percent'][key] > 0 for row in pairs))
                                     for key in pairs[0]['null_gains_percent']})
    summary[c['id']] = result
(p/'summary.json').write_text(json.dumps(summary, indent=2)+'\n')
(p/'diagnostic-status.json').write_text(json.dumps(dict(complete=True, processes=216, observations=1728,
    runtime_adopted=False, performance_qualification=False, full_objective_achieved=False), indent=2)+'\n')
lines = ['# Crossed source-control diagnostic', '',
         '216 processes / 1728 observations. Three labels per identical executable. Descriptive diagnostic only; original rejection remains binding.', '',
         '| Case | Series | Cross-build gain % | Positive rounds | Baseline null gains ab/ac/bc % | Candidate null gains ab/ac/bc % |',
         '| --- | --- | ---: | ---: | --- | --- |']
for case, series in summary.items():
    for name, r in series.items():
        null = lambda source: '/'.join(f"{r['nulls'][source+'_'+pair]['gain_percent']:.3f}" for pair in ['ab', 'ac', 'bc'])
        lines.append(f"| {case} | {name} | {r['cross_build_gain_percent']:.3f} | {r['positive_rounds']}/6 | {null('baseline')} | {null('candidate')} |")
(p/'report.md').write_text('\n'.join(lines)+'\n')
print('216 processes / 1728 observations analyzed; diagnostic only')
