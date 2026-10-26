from pathlib import Path
import subprocess,json,hashlib,os
p=Path(__file__).resolve().parent;meta=json.loads((p/'provenance.json').read_text());status=[]
for mode in ['baseline','candidate']:assert hashlib.sha256(Path(f'/tmp/lin-cow-row-{mode}').read_bytes()).hexdigest()==meta[mode+'_binary_sha256']
for i in range(1,13):
 modes=['baseline','control','candidate'];shift=i%3;modes=modes[shift:]+modes[:shift]
 for mode in modes:
  name=f'{i:02}-{mode}';binary=f'/tmp/lin-cow-row-{"candidate" if mode=="candidate" else "baseline"}';cmd=[binary,'--filter','compare/*_1row_*/*','--glob','--samples','8','--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output',str(p/name)]
  env=dict(os.environ,LIN_BENCH_SKIP_MYSQL='1',LIN_BENCH_SKIP_PG='1');env.pop('LIN_BENCH_MIXED_CASE',None)
  load=os.getloadavg()
  with (p/(name+'.log')).open('w')as log:r=subprocess.run(cmd,env=env,stdout=log,stderr=subprocess.STDOUT)
  status.append(dict(run=name,exit_code=r.returncode,load_before=load,load_after=os.getloadavg()));(p/'status.json').write_text(json.dumps(status,indent=2));assert r.returncode==0;print(name,0,flush=True)
