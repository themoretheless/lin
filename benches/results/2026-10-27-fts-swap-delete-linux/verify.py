"""Independent raw-observation check; no benchmark loader dependency."""
import json, statistics, math
from pathlib import Path
p=Path(__file__).resolve().parent
summary=json.loads((p/'summary.json').read_text())
count=0
for i in range(1,13):
    modes={}
    for mode in ('baseline','control','candidate'):
        d=json.loads((p/f'{i:02}-{mode}'/'run.json').read_text())
        assert d['status']=='complete' and len(d['observations'])==160
        vals={}
        for o in d['observations']:
            assert o['metric']=='wall' and o['operations']==1
            value=float(o['value']); assert math.isfinite(value) and value>0
            vals.setdefault(o['case'],[]).append(value)
        assert len(vals)==20 and all(len(v)==8 for v in vals.values())
        modes[mode]={k:statistics.median(v) for k,v in vals.items()}
        count+=160
    for case,a in modes['baseline'].items():
        series=summary[case]['series1' if i<=6 else 'series2']
        row=next(r for r in series['processes'] if r['round']==i)
        for key,mode in [('baseline_ns','baseline'),('control_ns','control'),('candidate_ns','candidate')]:
            assert row[key]==modes[mode][case]
        assert math.isclose(row['gain_percent'],100*(1-modes['candidate'][case]/a),abs_tol=1e-10)
print(f'PASS: {count} raw observations; 240 baseline/control/candidate comparisons')
