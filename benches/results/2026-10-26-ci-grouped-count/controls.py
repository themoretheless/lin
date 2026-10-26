from pathlib import Path
import subprocess,json,os,time,hashlib,re,statistics
p=Path(__file__).resolve().parent;meta=json.loads((p/'provenance.json').read_text());binary='/tmp/lin-ci-grouped-count-engine-baseline';assert hashlib.sha256(Path(binary).read_bytes()).hexdigest()==meta['baseline']['binary_sha256'];out=[]
for pair in range(1,4):
 for v in (['baseline','candidate'] if pair%2 else ['candidate','baseline']):
  name=f'resource-{pair}-{v}'
  command=['/usr/bin/time','-l','/tmp/lin-ci-grouped-count-engine-'+v,'--filter','compare/filter_eq/lin','--samples','1','--max-iterations','1','--warmup-ms','0','--output',str(p/name)]
  with (p/(name+'.log')).open('w') as log:r=subprocess.run(command,env=dict(os.environ,LIN_BENCH_SKIP_PG='1',LIN_BENCH_SKIP_MYSQL='1'),stdout=log,stderr=subprocess.STDOUT)
  assert r.returncode==0
  rss=int(re.search(r'(\d+)\s+maximum resident set size',(p/(name+'.log')).read_text())[1]);out.append(dict(pair=pair,variant=v,max_rss_bytes=rss))
  (p/'resources.json').write_text(json.dumps(out,indent=2));print(name,rss,flush=True)
q=p/'aa-controls';q.mkdir();(q/'provenance.json').write_text(json.dumps(dict(protocol='A/A: both labels execute the frozen baseline binary',binary_sha256=meta['baseline']['binary_sha256'],binary_source_sha256=meta['baseline']['source_sha256'],working_source_sha256=meta['candidate']['source_sha256']),indent=2));status={};loads=[]
for series in [1,2]:
 for family,pattern in [('point','compare/point_get/*'),('join','compare/join_inner/*')]:
  for pair in range(1,7):
   for label in (['baseline','candidate'] if pair%2 else ['candidate','baseline']):
    name=f's{series}-{family}-{label}-{pair}';loads.append(dict(name=name,unix_time=time.time(),load=os.getloadavg()));(q/'host-load.json').write_text(json.dumps(loads,indent=2))
    with (q/(name+'.log')).open('w') as log:r=subprocess.run([binary,'--filter',pattern,'--glob','--samples','24','--max-iterations','1024','--warmup-ms','5','--sample-ms','1','--output',str(q/name)],env=dict(os.environ,LIN_BENCH_SKIP_PG='1',LIN_BENCH_SKIP_MYSQL='1'),stdout=log,stderr=subprocess.STDOUT)
    status[name]=r.returncode;(q/'status.json').write_text(json.dumps(status,indent=2));assert r.returncode==0
  print(series,family,'AA complete',flush=True)
assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in meta['candidate']['source_sha256'].items())
