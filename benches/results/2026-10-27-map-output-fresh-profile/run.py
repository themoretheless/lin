from pathlib import Path
import subprocess,json,hashlib,os,platform
p=Path(__file__).resolve().parent;meta=json.loads((p/'provenance.json').read_text());binary=Path('/tmp/lin-map-output-fresh-profile');assert hashlib.sha256(binary.read_bytes()).hexdigest()==meta['binary_sha256'];status=[]
for i in range(1,4):
 for kind in (['update','delete']if i%2 else['delete','update']):
  name=f'{i:02}-{kind}';cmd=[str(binary),'--filter',f'compare/{kind}_1row_*/lin','--glob','--samples','8','--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output',str(p/name)]
  env=dict(os.environ,LIN_BENCH_SKIP_MYSQL='1',LIN_BENCH_SKIP_PG='1');env.pop('LIN_BENCH_MIXED_CASE',None);load=os.getloadavg()
  with(p/f'{name}.log').open('w')as log:r=subprocess.run(cmd,env=env,stdout=log,stderr=subprocess.STDOUT)
  status.append(dict(run=name,kind=kind,exit_code=r.returncode,load_before=load,load_after=os.getloadavg(),platform=platform.platform()));(p/'status.json').write_text(json.dumps(status,indent=2)+'\n');assert r.returncode==0
  d=json.loads((p/name/'run.json').read_text());assert d['status']=='complete'and len(d['cases'])==3 and len(d['observations'])==24;assert all(o['operations']==1 for o in d['observations'])
  records=[json.loads(s[6:])for s in(p/f'{name}.log').read_text().splitlines()if s.startswith('PHASE ')]
  assert {r['rows']for r in records}=={1000,10000,100000}and all(r['kind']==kind and len(r['stages_ns'])==25 and r['diagnostic_only']for r in records)
  assert all(sum(r['rows']==n for r in records)>=8 for n in [1000,10000,100000]);(p/f'{name}-phases.json').write_text(json.dumps(records,indent=2)+'\n');print(name,len(records),flush=True)
assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in meta['before_sha256'].items())
