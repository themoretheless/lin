from pathlib import Path
import json,re,statistics as st,hashlib
p=Path(__file__).resolve().parent;out={}
for mode in ['native','durable']:
 processes=[]
 for i in range(1,4):
  d=json.loads((p/f'{mode}-{i}/run.json').read_text());assert d['status']=='complete' and len(d['cases'])==1;assert len(d['observations'])==24 and all(x['operations']==1 for x in d['observations']);g={}
  for line in (p/f'{mode}-{i}.log').read_text().splitlines():
   m=re.search(r'(ROW_PHASE|MAINT_PHASE|EMBED_PHASE|WAL_PHASE|IO_PHASE|ENCODER_PHASE|COMMIT_PHASE) (.*)',line)
   if not m:continue
   v=dict(re.findall(r'(\w+)=(\w+)',m[2]))
   if m[1]=='WAL_PHASE':g.setdefault(v['phase']+'_ns',[]).append(int(v['ns']))
   else:
    for k,x in v.items():
     if k.endswith('_ns'):g.setdefault(k,[]).append(int(x))
  assert all(len(v)>=24 for v in g.values());processes.append({'events':{k:len(v) for k,v in g.items()},'medians':{k:st.median(v) for k,v in g.items()},'raw':g})
 out[mode]={'processes':processes,'median_of_process_medians':{k:st.median(x['medians'][k] for x in processes) for k in processes[0]['medians']}}
meta=json.loads((p/'provenance.json').read_text());assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in meta['before_sha256'].items())
(p/'phase-medians.json').write_text(json.dumps(out,indent=2)+'\n');print(json.dumps({k:v['median_of_process_medians'] for k,v in out.items()},indent=2))
