from pathlib import Path
import subprocess,json,hashlib
r=Path(__file__).resolve().parent;meta=json.loads((r/'provenance.json').read_text())
assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in meta['source_sha256'].items())
assert hashlib.sha256(Path('/tmp/lin-embedding-batch-allocation').read_bytes()).hexdigest()==json.loads((r/'binary-sha256.json').read_text())['sha256']
observations=[]
for case in ['default_1k','default_10k','unicode_1k','small_dim']:
 for i in range(1,4):
  with (r/f'{case}-p{i}.json').open('w') as out,(r/f'{case}-p{i}.log').open('w') as log:subprocess.run(['/tmp/lin-embedding-batch-allocation',case],stdout=out,stderr=log,check=True)
  d=json.loads((r/f'{case}-p{i}.json').read_text());assert len(d['warm_batches'])==12 and all(x['exact_bits'] for x in d['warm_batches']);observations.append(d)
summary={}
for case in sorted({d['case'] for d in observations}):
 ds=[d for d in observations if d['case']==case]
 for key in ['constructor','cold_batch','cold_shared_output_drop','cold_final_output_release']:assert all(d[key]==ds[0][key] for d in ds), (case,key)
 warms=[x for d in ds for x in d['warm_batches']];assert all(x['allocation']==warms[0]['allocation'] and x['release']==warms[0]['release'] for x in warms)
 summary[case]={**{k:ds[0][k] for k in ['rows','dim','constructor','cold_batch','cold_shared_output_drop','cold_final_output_release']},'warm_allocation':warms[0]['allocation'],'warm_release':warms[0]['release'],'warm_observations':len(warms),'exact_bits':True}
(r/'summary.json').write_text(json.dumps(summary,indent=2));print(summary)
assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in meta['source_sha256'].items())
