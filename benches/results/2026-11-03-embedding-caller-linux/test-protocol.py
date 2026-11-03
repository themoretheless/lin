"""Synthetic gate checks; never launches a timing executable."""
from pathlib import Path
import contextlib, io, json, statistics as st, tempfile

assets = Path(__file__).resolve().parent
plan = json.loads((assets/'plan.json').read_text())
source = (assets/'run.py').read_text()
analysis = source[source.index('assert len(records) == 324'):]
cases = ['insert_native', 'point_get', 'join_filter']
def run(kind):
    records = []
    for round in range(1, 13):
        for n in plan['sizes']:
            for case in cases:
                for role in ['baseline', 'control', 'candidate']:
                    value = 80 if role == 'candidate' and case == 'insert_native' else 100
                    if kind == 'target-loss' and n == 100000 and round == 1 and role == 'candidate' and case == 'insert_native':
                        value = 110
                    if kind == 'control-loss' and n == 1000 and round <= 6 and role == 'candidate' and case == 'point_get':
                        value = 110
                    records.append(dict(round=round, rows=n, case=case, role=role, median_ns=value))
    if kind == 'missing':
        records.pop()
    with tempfile.TemporaryDirectory() as tmp:
        out = Path(tmp)
        env = dict(records=records, plan=plan, cases=cases, out=out, st=st, json=json)
        with contextlib.redirect_stdout(io.StringIO()):
            exec(compile(analysis, 'gate-analysis', 'exec'), env)
        return json.loads((out/'decision.json').read_text()), json.loads((out/'summary.json').read_text())
decision, windows = run('pass')
assert decision['component_gate_passes'] and len(windows) == 18
assert sum(w['target'] for w in windows) == 6
for kind in ['target-loss', 'control-loss']:
    decision, windows = run(kind)
    assert not decision['component_gate_passes']
    assert sum(not w['passes'] for w in windows) == 1
try:
    run('missing')
except AssertionError:
    pass
else:
    raise AssertionError('missing record accepted')
print('Synthetic 18-window gate checks passed: clean, target loss, control loss, missing record')
