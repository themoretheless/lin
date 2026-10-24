from pathlib import Path
import subprocess,json,hashlib,statistics as st,os
r=Path(__file__).resolve().parent;meta=json.loads((r/'provenance.json').read_text());assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in meta['source_sha256'].items())
assert hashlib.sha256(Path('/tmp/lin-wal-maintenance-overlap-calibrated-gate').read_bytes()).hexdigest()==json.loads((r/'binary-sha256.json').read_text())['sha256'];out=[];status={}
for series in [1,2]:
 ds=[]
 for pair in range(1,7):
  i=(series-1)*6+pair;name=f'process-{i}';load=os.getloadavg()
  with (r/(name+'.jsonl')).open('w') as stdout,(r/(name+'.log')).open('w') as log:p=subprocess.run(['/tmp/lin-wal-maintenance-overlap-calibrated-gate',str(i)],stdout=stdout,stderr=log)
  status[str(i)]={'exit_code':p.returncode,'load_before':load};(r/'status.json').write_text(json.dumps(status,indent=2));assert p.returncode==0
  d=[json.loads(x) for x in (r/(name+'.jsonl')).read_text().splitlines()];assert len(d)==3 and all(x['exact_state_and_bytes'] and len(x['ns'])==3 and all(len(ns)==24 for ns in x['ns']) for x in d);ds.append(d)
 for case in ['default_1k','default_10k','small_127']:
  pairs=[list(map(st.median,next(x for x in d if x['case']==case)['ns'])) for d in ds]
  for v,label in [(1,'AA'),(2,'overlap')]:
   gains=[100*(1-x[v]/x[0]) for x in pairs];out.append({'series':series,'case':case,'comparison':label,'baseline_ns':st.median(x[0] for x in pairs),'candidate_ns':st.median(x[v] for x in pairs),'paired_gain_percent':st.median(gains),'median_absolute_difference_percent':st.median(abs(x) for x in gains),'wins':sum(x>0 for x in gains)})
 (r/'summary.json').write_text(json.dumps(out,indent=2));print(series,out[-6:],flush=True)
assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in meta['source_sha256'].items())
