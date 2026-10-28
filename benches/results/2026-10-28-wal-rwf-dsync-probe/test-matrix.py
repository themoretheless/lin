import tempfile,json,hashlib,copy
from pathlib import Path
from matrix import coordinates,key,analyze,check
smoke=Path(__file__).resolve().parent/'local-smoke'
with tempfile.TemporaryDirectory()as d:
 p=Path(d);(p/'raw').mkdir();saved=[]
 for c in coordinates():
  r=json.loads((smoke/f"{c['sync']}-{c['shape']}-{c['commits']}.json").read_text());r.update(mode=c['mode'],correctness_only=False,ns_per_commit=90 if c['mode']=='candidate'and c['sync']=='full'else 100)
  if c['mode']=='candidate'and c['sync']=='full':r['calls']=dict(write_all=0,sync_data=0,pwritev2=c['commits'],interrupted=0,partial=0)
  rows=[dict(r,sample=i)for i in range(8)];data=''.join(json.dumps(x)+'\n'for x in rows).encode();stem=p/'raw'/key(c);f=stem.with_suffix('.jsonl');f.write_bytes(data);m=stem.with_suffix('.meta.json');meta=dict(coordinate=c,exit=0,wall_seconds=1,stdout_sha256=hashlib.sha256(data).hexdigest());m.write_text(json.dumps(meta));saved.append((c,f,m,data,meta,rows))
 assert analyze(p)['io_gate_passes']
 c,f,m,data,meta,rows=saved[0]
 f.unlink()
 try:analyze(p);raise RuntimeError('missing process accepted')
 except FileNotFoundError:pass
 f.write_bytes(data)
 for field,value in [('ns_per_commit',-1),('exact_bytes',False),('replay_canonical_bits_verified',False),('sample',9)]:
  bad=copy.deepcopy(rows);bad[0][field]=value
  try:check(bad,c);raise RuntimeError('corruption accepted')
  except AssertionError:pass
 changed=copy.deepcopy(rows);changed[0]['stream_crc32_ieee']+=1;raw=''.join(json.dumps(x)+'\n'for x in changed).encode();f.write_bytes(raw);m.write_text(json.dumps(dict(meta,stdout_sha256=hashlib.sha256(raw).hexdigest())))
 try:analyze(p);raise RuntimeError('different stream accepted')
 except AssertionError:pass
 f.write_bytes(data);m.write_text(json.dumps(meta));assert analyze(p)['io_gate_passes']
print('complete synthetic 432-process matrix and six corruptions rejected; synthetic data is not performance evidence')
