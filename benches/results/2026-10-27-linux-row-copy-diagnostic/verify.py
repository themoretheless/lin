from pathlib import Path
import sys,json,math,statistics as st,hashlib
p=Path(sys.argv[1])if len(sys.argv)>1 else Path(__file__).resolve().parent
meta=json.loads((p/'input-provenance.json').read_text())
for name,digest in meta['diagnostic_files_sha256'].items():assert hashlib.sha256((p/name).read_bytes()).hexdigest()==digest
for name,digest in meta['candidate_sha256'].items():assert hashlib.sha256((p/f'map-source-{name}.rs').read_bytes()).hexdigest()==digest
provenance=json.loads((p/'provenance.json').read_text())
for name,digest in provenance['instrumented_sha256'].items():assert hashlib.sha256((p/('instrumented-'+Path(name).name)).read_bytes()).hexdigest()==digest
for name,digest in provenance['before_sha256'].items():assert digest==(meta['compare_sha256']if name=='benches/compare.rs'else meta['source_sha256'][name])
expected={f'{i:02}-{role}'for i in range(1,13)for role in ['baseline','control','candidate']}|{f'{i:02}-staged'for i in range(1,4)}
status=json.loads((p/'status.json').read_text());assert len(status)==39 and {r['run']for r in status}==expected
assert len({(r['pid'],r['started_unix'])for r in status})==39
assert all(r['exit_code']==0 and r['elapsed_seconds']>0 and r['copy_mode']==('staged'if r['run'].endswith('-staged')else'iter'if r['run'].endswith('-candidate')else'std')for r in status)
medians={};phase_count=wall_count=0
for name in sorted(expected):
 records=[json.loads(line[6:])for line in(p/f'{name}.log').read_text().splitlines()if line.startswith('PHASE ')]
 assert records==json.loads((p/f'{name}-phases.json').read_text())and len(records)==33;phase_count+=len(records)
 data=json.loads((p/name/'run.json').read_text());assert data['status']=='complete'and len(data['cases'])==3 and len(data['observations'])==24
 for label in ['1k','10k','100k']:
  observations=[o for o in data['observations']if o['case']==f'compare/update_1row_{label}/lin'];assert len(observations)==8
  assert all(o['metric']=='wall'and o['operations']==1 and math.isfinite(float(o['value']))and float(o['value'])>0 for o in observations)
 wall_count+=24
 assert all(r['kind']=='update'and r['diagnostic_only']and len(r['stages_ns'])==28 and all(isinstance(v,int)and v>=0 for v in r['stages_ns'])and r['prepared_ns']>0 and r['stages_ns'][15]>0 and r['stages_ns'][21]>0 for r in records)
 if name.endswith('-staged'):assert all(all(r['stages_ns'][idx]>0 for idx in [25,26,27])for r in records)
 else:assert all(all(r['stages_ns'][idx]==0 for idx in [25,26,27])for r in records)
 for n in [1000,10000,100000]:
  group=[r for r in records if r['rows']==n];assert len(group)==11
  medians[name,n]={key:st.median(r['prepared_ns']if idx is None else r['stages_ns'][idx]for r in group)for key,idx in [('prepared_ns',None),('copy_ns',21),('keys_ns',25),('values_ns',26),('build_ns',27)]}
s=json.loads((p/'summary.json').read_text());assert s['diagnostic_only']and s['phase_records']==phase_count==1287 and s['wall_observations']==wall_count==936
assert set(s['comparisons'])=={f'{n}/{metric}'for n in [1000,10000,100000]for metric in ['prepared_ns','copy_ns']}
accepted=True
for n in [1000,10000,100000]:
 for metric in ['prepared_ns','copy_ns']:
  reported=s['comparisons'][f'{n}/{metric}'];assert set(reported)=={'series1','series2'}
  for label,rounds in [('series1',range(1,7)),('series2',range(7,13))]:
   gains=[];noise=[];q=reported[label];assert len(q['processes'])==6 and {r['round']for r in q['processes']}==set(rounds)
   for i in rounds:
    a,b,c=(medians[f'{i:02}-{role}',n][metric]for role in ['baseline','control','candidate']);g=100*(1-c/a);aa=100*(1-b/a);gains.append(g);noise.append(abs(aa));r=next(r for r in q['processes']if r['round']==i)
    assert (r['baseline'],r['control'],r['candidate'],r['gain_percent'],r['aa_percent'])==(a,b,c,g,aa)
   gain=st.median(gains);aa=st.median(noise);positive=sum(g>0 for g in gains);passed=positive==6 and gain>aa;accepted &= passed
   assert (q['gain_percent'],q['aa_absolute_percent'],q['positive_pairs'],q['passes'])==(gain,aa,positive,passed)
 for metric in ['prepared_ns','copy_ns','keys_ns','values_ns','build_ns']:assert s['staged'][str(n)][metric]==st.median(medians[f'{i:02}-staged',n][metric]for i in range(1,4))
selection=json.loads((p/'selection.json').read_text());assert selection==dict(diagnostic_selection_qualified=accepted,engine_qualified=False,full_objective_achieved=False)
print(json.dumps(dict(processes=39,phase_records=phase_count,wall_observations=wall_count,source_hashes_verified=True,all_medians_verified=True,diagnostic_selection_qualified=accepted,engine_qualified=False,full_objective_achieved=False),indent=2))
