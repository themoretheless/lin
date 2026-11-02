from pathlib import Path
import hashlib,json,os,statistics,subprocess,sys,tempfile
p=Path(__file__).resolve().parent;root=p.parents[2];sys.path.insert(0,str(root/'scripts'))
from benchmark_role_balance import schedule
cases=[f'compare/durable_{op}_1row_{n}/{engine}'for n in ['1k','10k']for op in ['update','delete','commit','rollback']for engine in ['lin','sqlite_wal']]
cases +=[f'compare/update_1row_{n}/lin'for n in ['1k','10k','100k']]+['compare/insert_bulk_10k/lin','compare/fts_lex_selective/lin'];roles=['base','cont','cand'];checks=[]
for corruption in [None,'order','sample','binary','layout','summary','case_coverage']:
 with tempfile.TemporaryDirectory(prefix='lin-extent-audit-')as tmp:
  base=Path(tmp).resolve();protocol=base/'benches/results/fixture';protocol.mkdir(parents=True);q=protocol/'series-1';q.mkdir();(protocol/'verify.py').write_bytes((p/'verify.py').read_bytes())
  def put(path,value):path.parent.mkdir(parents=True,exist_ok=True);path.write_text(json.dumps(value))
  exe=base/'binary';exe.write_bytes(b'fixture executable');binary=dict(path=str(exe),sha256=hashlib.sha256(exe.read_bytes()).hexdigest())
  put(protocol/'input.json',dict(files_sha256={},runtime_reference='fixture'));put(protocol/'provenance.json',dict(source_restored=True,runtime_reference='fixture',binary=binary))
  for mode,passed,ignored in [('feature',476,4),('default',463,2)]:(protocol/(mode+'-tests.log')).write_text(f'test result: ok. {passed} passed; 0 failed; {ignored} ignored;\n')
  put(q/'environment.json',dict(source_sha256={}));put(q/'cases.json',cases)
  put(q/'status.json',dict(processes=378,observations=3024,smokes=42,measurement_complete=True,source_unchanged=True,one_binary=True,performance_qualified=False,adopted=False))
  def record(role,case,rnd=None):
   mode='extent'if role=='cand'and '/durable_'in case and case.endswith('/lin')else'legacy';n=1 if rnd is None else 8
   value=float(100+cases.index(case));value*=1.01 if role=='cont'else 0.75 if mode=='extent'else 1
   dest=q/'smokes'/f'{role}-{cases.index(case):02}'if rnd is None else q/'raw'/f'{rnd:02}-{role}'/case.replace('/','_')
   command=[str(exe),'--filter',case,'--samples',str(n),'--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output',str(dest)]
   put(dest/'run.json',dict(status='complete',cases=[dict(id=case)],observations=[dict(case=case,metric='wall',operations=1,availability=dict(state='available'),value=value)for _ in range(n)]))
   r=dict(role=role,case=case,layout=mode,command=command,exit_code=0,binary_sha256=binary['sha256'],median_ns=value)
   if rnd is not None:r['round']=rnd
   return r
  smokes=[record(role,case)for role in ['base','cand']for case in cases];records=[record(i['role'],i['workload'],i['round'])for i in schedule([dict(id=c)for c in cases],roles,rounds=6)];summary={}
  for case in cases:
   pairs=[]
   for rnd in range(1,7):
    by={r['role']:r['median_ns']for r in records if r['case']==case and r['round']==rnd};pairs.append(dict(round=rnd,**by,gain_percent=100*(1-by['cand']/by['base']),aa_percent=100*(1-by['cont']/by['base'])))
   summary[case]=dict(gain_percent=statistics.median(r['gain_percent']for r in pairs),aa_absolute_percent=statistics.median(abs(r['aa_percent'])for r in pairs),positive_pairs=sum(r['gain_percent']>0 for r in pairs),pairs=pairs)
  if corruption=='order':records[0],records[1]=records[1],records[0]
  elif corruption=='sample':
   f=Path(records[0]['command'][-1])/'run.json';d=json.loads(f.read_text());d['observations'][0]['value']=0;put(f,d)
  elif corruption=='binary':exe.write_bytes(b'replaced executable')
  elif corruption=='layout':records[0]['layout']='wrong'
  elif corruption=='summary':summary[cases[0]]['gain_percent']+=1
  elif corruption=='case_coverage':put(q/'cases.json',cases[:-1]+['compare/unrequested/lin'])
  put(q/'process-status.json',records);put(q/'smoke-status.json',smokes);put(q/'summary.json',summary)
  run=subprocess.run([sys.executable,str(protocol/'verify.py'),'1'],env=dict(os.environ,PYTHONPATH=str(root/'scripts')),capture_output=True,text=True)
  assert(run.returncode==0)==(corruption is None),(corruption,run.stdout,run.stderr)
  checks.append(dict(corruption=corruption,exit_code=run.returncode,expected_rejection=corruption is not None))
(p/'audit-fixtures.json').write_text(json.dumps(dict(synthetic_tests=7,actual_measurements=0,all_passed=True,checks=checks),indent=2)+'\n');print('PASS complete 3024-observation fixture and six corruption rejections')
