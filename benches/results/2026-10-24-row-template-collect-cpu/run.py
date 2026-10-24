from pathlib import Path
import subprocess,json,hashlib,statistics as st
r=Path(__file__).resolve().parent
meta=json.loads((r/'provenance.json').read_text())
assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in meta['source_sha256'].items())
assert hashlib.sha256(Path('/tmp/lin-row-template-collect-gate').read_bytes()).hexdigest()==json.loads((r/'binary-sha256.json').read_text())['sha256']
status={};summary=[]
for series in [1,2]:
 observations=[]
 for pair in range(1,7):
  i=(series-1)*6+pair
  with (r/f'process-{i}.jsonl').open('w') as out,(r/f'process-{i}.log').open('w') as log:p=subprocess.run(['/tmp/lin-row-template-collect-gate',str(i)],stdout=out,stderr=log)
  status[str(i)]=p.returncode;(r/'status.json').write_text(json.dumps(status,indent=2));assert p.returncode==0
  d=[json.loads(x) for x in (r/f'process-{i}.jsonl').read_text().splitlines()];assert len(d)==4 and all(x['exact_rows'] and len(x['ns'])==2 and all(len(t)==24 for t in x['ns']) for x in d);observations.append(d)
 for case in [x['case'] for x in observations[0]]:
  pairs=[list(map(st.median,next(x for x in d if x['case']==case)['ns'])) for d in observations]
  summary.append({'series':series,'case':case,'baseline_ns':st.median(x[0] for x in pairs),'candidate_ns':st.median(x[1] for x in pairs),'paired_gain_percent':st.median((1-b/a)*100 for a,b in pairs),'wins':sum(b<a for a,b in pairs)})
 (r/'summary.json').write_text(json.dumps(summary,indent=2));print(series,summary[-4:],flush=True)
assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in meta['source_sha256'].items())
