from pathlib import Path
import subprocess,json,hashlib,statistics as st
r=Path(__file__).resolve().parent;meta=json.loads((r/'provenance.json').read_text())
assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in meta['source_sha256'].items())
assert hashlib.sha256(Path('/tmp/lin-embedding-fill-finish-profile').read_bytes()).hexdigest()==json.loads((r/'binary-sha256.json').read_text())['sha256']
processes=[]
for i in range(1,4):
 with (r/f'process-{i}.jsonl').open('w') as out,(r/f'process-{i}.log').open('w') as log:subprocess.run(['/tmp/lin-embedding-fill-finish-profile'],stdout=out,stderr=log,check=True)
 ds=[json.loads(x) for x in (r/f'process-{i}.jsonl').read_text().splitlines()];assert len(ds)==48
 p={}
 for case in sorted({x['case'] for x in ds}):
  obs=[x for x in ds if x['case']==case];assert {x['sample'] for x in obs}==set(range(12)) and all(x['exact_bits_preflight'] for x in obs)
  pairs=[p for x in obs for p in x['sampled_fill_finish_ns']];assert all(a>=0 and b>=0 for a,b in pairs)
  p[case]={'fill_ns_per_sampled_row':st.median(a for a,b in pairs),'finish_ns_per_sampled_row':st.median(b for a,b in pairs),'batch_ns':st.median(x['batch_ns_excluding_output_drop'] for x in obs),'sampled_rows':len(pairs)}
 processes.append(p);print(i,'complete',flush=True)
summary={case:{k:st.median(p[case][k] for p in processes) for k in processes[0][case]} for case in processes[0]}
(r/'summary.json').write_text(json.dumps({'processes':processes,'median_of_process_medians':summary},indent=2));print(summary)
assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in meta['source_sha256'].items())
