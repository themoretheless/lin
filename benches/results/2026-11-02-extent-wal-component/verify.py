from pathlib import Path
import hashlib,json,math,re,statistics,sys
protocol=Path(__file__).resolve().parent;root=protocol.parents[2];sys.path.insert(0,str(root/'scripts'))
from benchmark_role_balance import schedule,orders_from_records
assert sys.argv[1:]in[['1'],['2']];p=protocol/('series-'+sys.argv[1]);load=lambda name:json.loads((p/name).read_text());sha=lambda b:hashlib.sha256(b).hexdigest()
m=json.loads((protocol/'provenance.json').read_text());inputs=json.loads((protocol/'input.json').read_text());assert m['source_restored']and m['runtime_reference']==inputs['runtime_reference']
for name,h in inputs['files_sha256'].items():assert sha((root/name).read_bytes())==h,name
for mode,expected,ignored in [('feature',476,4),('default',463,2)]:
 counts=re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',(protocol/(mode+'-tests.log')).read_text());assert [sum(int(r[i])for r in counts)for i in range(3)]==[expected,0,ignored]
assert sha(Path(m['binary']['path']).read_bytes())==m['binary']['sha256']
cases=load('cases.json');records=load('process-status.json');smokes=load('smoke-status.json');reported=load('summary.json');roles=['base','cont','cand']
assert len(cases)==21 and len(set(cases))==21 and len(records)==378 and len(smokes)==42
expected=[f'compare/durable_{op}_1row_{n}/{engine}'for n in ['1k','10k']for op in ['update','delete','commit','rollback']for engine in ['lin','sqlite_wal']]
expected +=[f'compare/update_1row_{n}/lin'for n in ['1k','10k','100k']]+['compare/insert_bulk_10k/lin','compare/fts_lex_selective/lin']
assert cases==expected
assert load('status.json')==dict(processes=378,observations=3024,smokes=42,measurement_complete=True,source_unchanged=True,one_binary=True,performance_qualified=False,adopted=False)
plan=schedule([dict(id=c)for c in cases],roles,rounds=6);assert [(r['round'],r['role'],r['case'])for r in records]==[(r['round'],r['role'],r['workload'])for r in plan]
orders_from_records(records,cases,roles,'case',rounds=6)
assert {(r['role'],r['case'])for r in smokes}=={(role,c)for role in ['base','cand']for c in cases}
med={};count=0
for smoke,rows in [(True,smokes),(False,records)]:
 for r in rows:
  mode='extent'if r['role']=='cand'and '/durable_'in r['case']and r['case'].endswith('/lin')else'legacy';assert r['layout']==mode and r['exit_code']==0 and r['binary_sha256']==m['binary']['sha256']
  n=1 if smoke else 8;dest=p/'smokes'/f"{r['role']}-{cases.index(r['case']):02}"if smoke else p/'raw'/f"{r['round']:02}-{r['role']}"/r['case'].replace('/','_')
  assert r['command']==[m['binary']['path'],'--filter',r['case'],'--samples',str(n),'--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output',str(dest)]
  d=json.loads((dest/'run.json').read_text());assert d['status']=='complete'and len(d['cases'])==1 and d['cases'][0]['id']==r['case']and len(d['observations'])==n
  values=[]
  for o in d['observations']:
   assert o['case']==r['case']and o['metric']=='wall'and o['operations']==1 and o['availability']['state']=='available';v=float(o['value']);assert math.isfinite(v)and v>0;values.append(v)
  median=statistics.median(values);assert median==r['median_ns']
  if not smoke:med[r['round'],r['role'],r['case']]=median;count+=len(values)
rebuilt={}
for case in cases:
 pairs=[]
 for rnd in range(1,7):
  a,b,c=(med[rnd,role,case]for role in roles);pairs.append(dict(round=rnd,base=a,cont=b,cand=c,gain_percent=100*(1-c/a),aa_percent=100*(1-b/a)))
 rebuilt[case]=dict(gain_percent=statistics.median(r['gain_percent']for r in pairs),aa_absolute_percent=statistics.median(abs(r['aa_percent'])for r in pairs),positive_pairs=sum(r['gain_percent']>0 for r in pairs),pairs=pairs)
assert count==3024 and rebuilt==reported
assert load('environment.json')['source_sha256']=={str(f.relative_to(root)):sha(f.read_bytes())for f in(root/'src').rglob('*.rs')}
peers=[]
for n in ['1k','10k']:
 for op in ['update','delete','commit','rollback']:
  lin=f'compare/durable_{op}_1row_{n}/lin';sql=f'compare/durable_{op}_1row_{n}/sqlite_wal'
  for rnd in range(1,7):peers.append(dict(rows=n,operation=op,round=rnd,lin_ns=med[rnd,'cand',lin],sqlite_ns=med[rnd,'base',sql],lin_over_sqlite=med[rnd,'cand',lin]/med[rnd,'base',sql]))
(p/'audit.json').write_text(json.dumps(dict(audit_complete=True,processes=378,observations=count,smokes=42,one_binary=True,raw_recomputed=True,sqlite_full_wal_comparisons=peers,sqlite_wins=sum(r['lin_over_sqlite']<1 for r in peers),sqlite_comparisons=48,performance_qualified=False,adopted=False),indent=2)+'\n')
print('PASS 378 processes / 3024 observations; SQLite FULL WAL wins',sum(r['lin_over_sqlite']<1 for r in peers),'/48')
