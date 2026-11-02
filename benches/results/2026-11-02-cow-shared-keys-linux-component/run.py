from pathlib import Path
import hashlib,json,math,os,platform,statistics,subprocess,sys,time
protocol=Path(__file__).resolve().parent;root=protocol.parents[2]
p=protocol/('series-'+sys.argv[1]);assert sys.argv[1:] in [['1'],['2']];p.mkdir(exist_ok=False)
sys.path.insert(0,str(root/'scripts'))
from benchmark_role_balance import schedule
sha=lambda b:hashlib.sha256(b).hexdigest()
m=json.loads((protocol/'provenance.json').read_text());assert m['source_restored'] and 'candidate'in m
cases=[f'compare/{operation}_1row_{n}/lin'for operation in ['update','tx_rollback']for n in ['1k','10k','100k']]
cases+=['compare/insert_bulk_10k/lin','compare/delete_1row_10k/lin','compare/fts_lex_selective/lin','compare/agg_count_by/duckdb','compare/durable_update_1row_10k/lin','compare/durable_commit_1row_1k/lin']
assert len(cases)==12;plan=schedule([dict(id=c)for c in cases],['baseline','control','candidate'],rounds=6);assert len(plan)==216
(p/'cases.json').write_text(json.dumps(cases,indent=2)+'\n')
assert not(p/'process-status.json').exists() and not(p/'smoke-status.json').exists(),'retain earlier data'
original={str(f.relative_to(root)):sha(f.read_bytes())for f in(root/'src').rglob('*.rs')}
env=dict(os.environ,LIN_BENCH_SKIP_MYSQL='1',LIN_BENCH_SKIP_PG='1',LC_ALL='C')
for key in ['LIN_BENCH_MIXED_CASE','LIN_COLUMN_BATCH_FIXTURE']:env.pop(key,None)
(p/'environment.json').write_text(json.dumps(dict(platform=platform.platform(),local_discovery_only=True,linux_qualification=False,component_experiment=True,source_sha256=original),indent=2)+'\n')

def execute(role,case,dest,samples):
 b=m['candidate' if role=='candidate' and not case.endswith('/duckdb') else 'baseline']
 assert sha(Path(b['path']).read_bytes())==b['sha256'];dest.parent.mkdir(parents=True,exist_ok=True)
 command=[b['path'],'--filter',case,'--samples',str(samples),'--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output',str(dest)]
 start=time.time()
 with dest.with_suffix('.log').open('w')as log:r=subprocess.run(command,cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT)
 record=dict(role=role,case=case,command=command,exit_code=r.returncode,binary_sha256=b['sha256'],elapsed_seconds=time.time()-start)
 assert r.returncode==0,record
 raw=json.loads((dest/'run.json').read_text());assert raw['status']=='complete'and len(raw['cases'])==1 and raw['cases'][0]['id']==case and len(raw['observations'])==samples
 values=[]
 for o in raw['observations']:
  assert o['case']==case and o['metric']=='wall'and o['operations']==1 and o['availability']['state']=='available'
  value=float(o['value']);assert math.isfinite(value)and value>0;values.append(value)
 record['median_ns']=statistics.median(values);return record
smokes=[]
for role in ['baseline','candidate']:
 for i,case in enumerate(cases):
  smokes.append(execute(role,case,p/'smokes'/f'{role}-{i:02}',1))
  (p/'smoke-status.json').write_text(json.dumps(smokes,indent=2)+'\n')
records=[]
for item in plan:
 r=execute(item['role'],item['workload'],p/'raw'/f"{item['round']:02}-{item['role']}"/item['workload'].replace('/','_'),8)
 r['round']=item['round'];records.append(r)
 (p/'process-status.json').write_text(json.dumps(records,indent=2)+'\n')
 print(item['round'],item['role'],item['workload'],flush=True)
assert original=={str(f.relative_to(root)):sha(f.read_bytes())for f in(root/'src').rglob('*.rs')}
summary={}
for case in cases:
 pairs=[]
 for rnd in range(1,7):
  by={r['role']:r['median_ns']for r in records if r['case']==case and r['round']==rnd};assert len(by)==3
  pairs.append(dict(round=rnd,**by,gain_percent=100*(1-by['candidate']/by['baseline']),aa_percent=100*(1-by['control']/by['baseline'])))
 summary[case]=dict(gain_percent=statistics.median(r['gain_percent']for r in pairs),aa_absolute_percent=statistics.median(abs(r['aa_percent'])for r in pairs),positive_pairs=sum(r['gain_percent']>0 for r in pairs),pairs=pairs)
(p/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
(p/'status.json').write_text(json.dumps(dict(processes=216,observations=1728,smokes=24,measurement_complete=True,source_restored=True,performance_qualification=False,adopted=False),indent=2)+'\n')
print(json.dumps(dict(complete=True,performance_qualification=False)))
