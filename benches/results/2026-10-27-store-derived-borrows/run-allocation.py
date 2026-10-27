from pathlib import Path
import json,subprocess,hashlib,statistics as st
p=Path(__file__).resolve().parent;m=json.loads((p/'allocation-provenance.json').read_text());status=[];data={}
for i in range(1,4):
 for role in (['baseline','candidate']if i%2 else['candidate','baseline']):
  b=m['binaries'][role];assert hashlib.sha256(Path(b['path']).read_bytes()).hexdigest()==b['sha256']
  with(p/f'alloc-{i:02}-{role}.jsonl').open('w')as out,(p/f'alloc-{i:02}-{role}.log').open('w')as err:r=subprocess.run([b['path']],stdout=out,stderr=err)
  status.append(dict(round=i,role=role,exit_code=r.returncode));(p/'allocation-status.json').write_text(json.dumps(status,indent=2)+'\n');assert r.returncode==0
  rows=[json.loads(l)for l in(p/f'alloc-{i:02}-{role}.jsonl').read_text().splitlines()];assert len(rows)==9
  for row in rows:
   assert row['exact_readback'] and row['full_output_validated'] and row['handle_drop_included'];data.setdefault((row['rows'],row['case'],role),[]).append(row)
summary={}
for n in [1000,10000,100000]:
 for case in ['wing','title','delete']:
  out={role:{k:st.median(r[k]for r in data[n,case,role])for k in ['allocations','requested_bytes','deallocations']}for role in ['baseline','candidate']}
  out['difference']={k:out['candidate'][k]-out['baseline'][k]for k in out['baseline']};summary[f'{n}/{case}']=out
(p/'allocation-summary.json').write_text(json.dumps(dict(processes=6,observations=54,latency_measured=False,rss_measured=False,results=summary),indent=2)+'\n');print(json.dumps(summary,indent=2))
assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in m['source_sha256'].items())
