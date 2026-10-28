from pathlib import Path
import json, statistics, hashlib
root=Path('benches/results/2026-10-28-point-values-local-baseline')
rows=[]
for n in range(1,4):
 j=json.loads((root/f'process-{n}/run.json').read_text())
 assert j['status']=='complete' and len(j['cases'])==6 and len(j['observations'])==72
 values={c['id']:[] for c in j['cases']}
 for o in j['observations']:
  assert o['availability']['state']=='available' and o['operations']>0
  assert o['operations']<=8 and o['metric']=='wall'
  values[o['case']].append(float(o['value'])/o['operations'])
 assert all(len(v)==12 for v in values.values())
 for case,v in values.items():rows.append(dict(process=n,case=case,median_ns=statistics.median(v),samples=len(v)))
comparisons=[]
for kind in ['hit','miss']:
 for peer in ['sqlite','duckdb']:
  ratios=[]
  for n in range(1,4):
   lin=next(x['median_ns'] for x in rows if x['process']==n and x['case']==f'point_values/{kind}/lin')
   other=next(x['median_ns'] for x in rows if x['process']==n and x['case']==f'point_values/{kind}/{peer}')
   ratios.append(lin/other)
  comparisons.append(dict(kind=kind,peer=peer,lin_over_peer=ratios,numerical_wins=sum(x<1 for x in ratios)))
summary=dict(measurement_complete=True,processes=3,cases=6,observations=216,performance_qualified=False,scope='macOS local capped diagnostic; 8 operations maximum, short sample durations; no A/A or source qualification',medians=rows,comparisons=comparisons)
(root/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
text=['# Equivalent point-values local baseline','','Runtime a7d4b02c3; harness 63d8a7f53; macOS ARM64 nightly 1.101. Three fresh sequential processes, six cases, twelve samples, maximum eight operations per sample. Full value validation before calibration and after measurement. Input setup/drop excluded, output drop included. All raw cases retained.','','This is a capped local diagnostic. Sub-microsecond operations, scheduler effects, and short samples limit precision. Numerical ratios are not performance qualification, evidence of production COW acceptance, or completion of the eight-peer goal. No A/A noise gate.','','| Case | Peer | Lin/peer per process | Numerical wins |','|---|---|---|---|']
for c in comparisons:text.append('| '+c['kind']+' | '+c['peer']+' | '+', '.join(f'{x:.4f}' for x in c['lin_over_peer'])+' | '+str(c['numerical_wins'])+'/3 |')
text+=['','All engines return owned id/title strings and preserve multiplicity. Legacy COUNT-based point_get results remain separate. These results do not cover PostgreSQL/MySQL/MongoDB/pandas/MSSQL/Kusto, durable writes, or the full engine control matrix.']
(root/'report.md').write_text('\n'.join(text)+'\n')
manifest={str(p.relative_to(root)):hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(root.rglob('*')) if p.is_file() and p.name!='sha256.json'}
(root/'sha256.json').write_text(json.dumps(manifest,indent=2)+'\n')
print(json.dumps(comparisons))
