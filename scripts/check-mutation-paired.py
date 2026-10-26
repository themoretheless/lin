#!/usr/bin/env python3
"""Experimental candidate gate; all peer comparisons remain visible, scope is native mutation."""
import json,sys
from pathlib import Path
p=Path(sys.argv[1]);summary=json.loads((p/'summary.json').read_text());status=json.loads((p/'status.json').read_text())
assert len(status)==36 and all(r['exit_code']==0 for r in status)
assert len(summary)==20 and all(set(v)=={'series1','series2'} for v in summary.values())
assert all(s['pairs']==6 for v in summary.values()for s in v.values())
accepted=True;report=['# Paired native mutation qualification','', 'Two six-round series; target update1k/10k/100k must win6/6 and exceed median absolute A/A. All unchanged controls and SQLite losses retained. This is not eight-peer or Full durable qualification.','', '| Case | Series | Gain % | A/A % | Positive pairs | SQLite wins |','|---|---|---:|---:|---:|---:|']
for case,series in summary.items():
 for label,s in series.items():
  target=case.startswith('compare/update_1row_') and case.endswith('/lin')
  if target:accepted &= s['positive_pairs']==6 and s['gain_percent']>s['aa_absolute_percent']
  else:accepted &= s['gain_percent'] >= -max(2.0,s['aa_absolute_percent'])
  report.append(f"| {case} | {label} | {s['gain_percent']:.3f} | {s['aa_absolute_percent']:.3f} | {s['positive_pairs']}/6 | {s.get('sqlite_wins','n/a')} |")
result={'candidate_qualified':bool(accepted),'complete_rounds':12,'full_objective_achieved':False,'scope':'native mutation only; peer wins listed separately'}
(p/'qualification.json').write_text(json.dumps(result,indent=2)+'\n');report+=['',f'Candidate qualified: {accepted}. All-eight-peer goal remains open.'];(p/'paired-report.md').write_text('\n'.join(report)+'\n');print('\n'.join(report));sys.exit(0 if accepted else 3)
