from pathlib import Path
import subprocess,json,hashlib,os
p=Path(__file__).resolve().parent;meta=json.loads((p/'provenance.json').read_text());processes=[]
CASES=['compare/update_1row_1k/lin', 'compare/update_1row_1k/sqlite', 'compare/delete_1row_1k/lin', 'compare/delete_1row_1k/sqlite', 'compare/update_1row_10k/lin', 'compare/update_1row_10k/sqlite', 'compare/delete_1row_10k/lin', 'compare/delete_1row_10k/sqlite', 'compare/update_1row_100k/lin', 'compare/update_1row_100k/sqlite', 'compare/delete_1row_100k/lin', 'compare/delete_1row_100k/sqlite', 'compare/tx_commit_1row_1k/lin', 'compare/tx_commit_1row_1k/sqlite', 'compare/tx_rollback_1row_1k/lin', 'compare/tx_rollback_1row_1k/sqlite', 'compare/tx_commit_1row_10k/lin', 'compare/tx_commit_1row_10k/sqlite', 'compare/tx_rollback_1row_10k/lin', 'compare/tx_rollback_1row_10k/sqlite']
for mode in ['baseline','candidate']:assert hashlib.sha256(Path(f'/tmp/lin-cow-inplace-{mode}').read_bytes()).hexdigest()==meta[mode+'_binary_sha256']
for round in range(1,13):
 cases=CASES[round%20:]+CASES[:round%20]
 for case_index,case in enumerate(cases):
  modes=['baseline','control','candidate'];shift=(round+case_index)%3;modes=modes[shift:]+modes[:shift]
  for mode in modes:
   group=f'{round:02}-{mode}';name=case.replace('/','_');dest=p/group/name
   binary=f'/tmp/lin-cow-inplace-{"candidate" if mode=="candidate"else"baseline"}'
   cmd=[binary,'--filter',case,'--samples','8','--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output',str(dest)]
   env=dict(os.environ,LIN_BENCH_SKIP_MYSQL='1',LIN_BENCH_SKIP_PG='1');env.pop('LIN_BENCH_MIXED_CASE',None)
   (p/group).mkdir(exist_ok=True);load=os.getloadavg()
   with (p/group/(name+'.log')).open('w')as log:r=subprocess.run(cmd,env=env,stdout=log,stderr=subprocess.STDOUT)
   processes.append(dict(round=round,mode=mode,case=case,exit_code=r.returncode,load_before=load,load_after=os.getloadavg()))
   (p/'process-status.json').write_text(json.dumps(processes,indent=2)+'\n');assert r.returncode==0
   d=json.loads((dest/'run.json').read_text());assert d['status']=='complete' and len(d['cases'])==1 and len(d['observations'])==8
   assert all(o['case']==case and o['operations']==1 for o in d['observations'])
  print(round,case,'complete',flush=True)
 status=[]
 for i in range(1,round+1):
  for mode in ['baseline','control','candidate']:
   group=[x for x in processes if x['round']==i and x['mode']==mode];assert len(group)==20
   status.append(dict(run=f'{i:02}-{mode}',exit_code=0,subprocesses=20))
 (p/'status.json').write_text(json.dumps(status,indent=2)+'\n')
assert len(processes)==720
