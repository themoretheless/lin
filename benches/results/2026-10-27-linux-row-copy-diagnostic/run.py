from pathlib import Path
import subprocess,json,hashlib,os,time,platform
p=Path(__file__).resolve().parent;meta=json.loads((p/'provenance.json').read_text());binary=Path('/tmp/lin-row-copy-diagnostic');assert hashlib.sha256(binary.read_bytes()).hexdigest()==meta['binary_sha256'];status=[]
(p/'environment.json').write_text(json.dumps(dict(platform=platform.platform(),cpu_count=os.cpu_count(),cpu_info=Path('/proc/cpuinfo').read_text().split('\n\n')[0]if Path('/proc/cpuinfo').exists()else None,rustc=subprocess.check_output(['rustc','-Vv'],text=True)),indent=2)+'\n')
def run(name,role,mode):
 cmd=[str(binary),'--filter','compare/update_1row_*/lin','--glob','--samples','8','--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output',str(p/name)]
 env=dict(os.environ,LIN_DIAGNOSTIC_ROW_COPY=mode,LIN_BENCH_SKIP_MYSQL='1',LIN_BENCH_SKIP_PG='1');env.pop('LIN_BENCH_MIXED_CASE',None);load=os.getloadavg();started=time.time()
 with(p/f'{name}.log').open('w')as log:
  child=subprocess.Popen(cmd,env=env,stdout=log,stderr=subprocess.STDOUT);pid=child.pid;code=child.wait()
 status.append(dict(run=name,role=role,copy_mode=mode,pid=pid,exit_code=code,started_unix=started,elapsed_seconds=time.time()-started,load_before=load,load_after=os.getloadavg()));(p/'status.json').write_text(json.dumps(status,indent=2)+'\n');assert code==0
 data=json.loads((p/name/'run.json').read_text());assert data['status']=='complete'and len(data['cases'])==3 and len(data['observations'])==24;assert {o['case']for o in data['observations']}=={f'compare/update_1row_{n}/lin'for n in ['1k','10k','100k']}and all(o['operations']==1 for o in data['observations'])
 rows=[json.loads(line[6:])for line in(p/f'{name}.log').read_text().splitlines()if line.startswith('PHASE ')];assert len(rows)==33 and all(r['kind']=='update'and r['diagnostic_only']and len(r['stages_ns'])==28 and r['stages_ns'][21]>0 for r in rows)
 assert all(sum(r['rows']==n for r in rows)==11 for n in [1000,10000,100000])
 if mode=='staged':assert all(all(r['stages_ns'][i]>0 for i in [25,26,27])for r in rows)
 else:assert all(all(r['stages_ns'][i]==0 for i in [25,26,27])for r in rows)
 (p/f'{name}-phases.json').write_text(json.dumps(rows,indent=2)+'\n');print(name,mode,'complete',flush=True)
for i in range(1,13):
 modes=['baseline','control','candidate'];shift=i%3;modes=modes[shift:]+modes[:shift]
 for role in modes:run(f'{i:02}-{role}',role,'iter'if role=='candidate'else'std')
for i in range(1,4):run(f'{i:02}-staged','decomposition','staged')
assert hashlib.sha256(binary.read_bytes()).hexdigest()==meta['binary_sha256'];assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in meta['before_sha256'].items())
