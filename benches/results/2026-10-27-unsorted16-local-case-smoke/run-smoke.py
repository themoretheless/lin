from pathlib import Path
import subprocess,json,os,hashlib,platform
src=Path('benches/results/2026-10-27-unsorted16-untraced-isolated');p=Path('benches/results/2026-10-27-unsorted16-local-case-smoke');p.mkdir(exist_ok=True);binary=Path('/tmp/lin-unsorted16-untraced');env=dict(os.environ,LIN_BENCH_SKIP_PG='1',LIN_BENCH_SKIP_MYSQL='1');env.pop('LIN_BENCH_MIXED_CASE',None);rows=[]
for case in json.loads((src/'cases.json').read_text()):
 name=case.replace('/','_');dest=p/name
 with(p/(name+'.log')).open('w')as log:r=subprocess.run([str(binary),'--filter',case,'--samples','1','--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output',str(dest)],env=env,stdout=log,stderr=subprocess.STDOUT)
 assert r.returncode==0,(case,r.returncode)
 d=json.loads((dest/'run.json').read_text());assert d['status']=='complete'and len(d['cases'])==1 and len(d['observations'])==1
 o=d['observations'][0];assert o['case']==case and o['operations']==1 and o['metric']=='wall'
 rows.append(dict(case=case,exit_code=r.returncode,observations=1));print(case,'PASS',flush=True)
(p/'verification.json').write_text(json.dumps(dict(platform=platform.platform(),binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),cases=rows,correctness_smoke_only=True,runtime_treatment_tested=False,linux_gate_evidence=False),indent=2)+'\n')
