from pathlib import Path
import subprocess,json,hashlib,statistics as st
r=Path(__file__).resolve().parent;meta=json.loads((r/'provenance.json').read_text());assert meta['baseline']['rustc']==meta['candidate']['rustc']
assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in meta['candidate']['source_sha256'].items())
for v in ['baseline','candidate']:assert hashlib.sha256(Path('/tmp/lin-bitmap-sample-once-'+v).read_bytes()).hexdigest()==meta[v]['binary_sha256']
summary=[];status={}
for series in [1,2]:
 data={}
 for pair in range(1,7):
  for v in (['baseline','candidate'] if pair%2 else ['candidate','baseline']):
   name=f's{series}-p{pair}-{v}'
   with (r/(name+'.jsonl')).open('w') as out,(r/(name+'.log')).open('w') as log:p=subprocess.run(['/tmp/lin-bitmap-sample-once-'+v],stdout=out,stderr=log)
   status[name]=p.returncode;(r/'status.json').write_text(json.dumps(status,indent=2));assert p.returncode==0
   ds=[json.loads(x) for x in (r/(name+'.jsonl')).read_text().splitlines()];assert len(ds)==4 and all(x['exact_postings'] and len(x['ns'])==12 and all(n>0 for n in x['ns']) for x in ds)
   for d in ds:data.setdefault(d['case'],{}).setdefault(v,[]).append(st.median(d['ns']))
 for case,vs in data.items():
  a=vs['baseline'];b=vs['candidate'];summary.append({'series':series,'case':case,'baseline_ns':st.median(a),'candidate_ns':st.median(b),'paired_gain_percent':st.median((1-y/x)*100 for x,y in zip(a,b)),'wins':sum(y<x for x,y in zip(a,b))})
 (r/'summary.json').write_text(json.dumps(summary,indent=2));print(series,summary[-4:],flush=True)
assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in meta['candidate']['source_sha256'].items())
