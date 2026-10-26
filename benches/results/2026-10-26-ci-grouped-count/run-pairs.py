from pathlib import Path
import subprocess,json,os,time,hashlib
p=Path(__file__).resolve().parent;meta=json.loads((p/'provenance.json').read_text());status={};loads=[]
assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in meta['candidate']['source_sha256'].items())
for v in ['baseline','candidate']:assert hashlib.sha256(Path('/tmp/lin-ci-grouped-count-engine-'+v).read_bytes()).hexdigest()==meta[v]['binary_sha256']
for series in [1,2]:
 for family,pattern in [('grouped','compare/filter*/*'),('point','compare/point_get/*'),('substring','compare/text_substr/*'),('join','compare/join_inner/*')]:
  for pair in range(1,7):
   for v in (['baseline','candidate'] if pair%2 else ['candidate','baseline']):
    name=f's{series}-{family}-{v}-{pair}';loads.append(dict(name=name,time=time.time(),load=os.getloadavg()));(p/'host-load.json').write_text(json.dumps(loads,indent=2))
    command=['/tmp/lin-ci-grouped-count-engine-'+v,'--filter',pattern,'--glob','--samples','24','--max-iterations','1024','--warmup-ms','5','--sample-ms','1','--output',str(p/name)]
    with (p/(name+'.log')).open('w') as log:r=subprocess.run(command,env=dict(os.environ,LIN_BENCH_SKIP_PG='1',LIN_BENCH_SKIP_MYSQL='1'),stdout=log,stderr=subprocess.STDOUT)
    status[name]=r.returncode;(p/'status.json').write_text(json.dumps(status,indent=2));print(name,r.returncode,flush=True);assert r.returncode==0
assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in meta['candidate']['source_sha256'].items())
