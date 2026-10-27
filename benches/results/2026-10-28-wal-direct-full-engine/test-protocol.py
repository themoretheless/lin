from pathlib import Path
import tempfile,json,shutil,subprocess,hashlib
src=Path(__file__).resolve().parent
with tempfile.TemporaryDirectory(prefix='lin-wal-reserve-protocol-')as tmp:
 p=Path(tmp)
 for n in ['summarize.py','verify.py','qualify.py','cases.json','candidate-persist.rs','compare.rs']:shutil.copy2(src/n,p/n)
 def write(n,obj):(p/n).write_text(json.dumps(obj)+'\n')
 def run(n,ok=True):
  r=subprocess.run(['python3',str(p/n)],capture_output=True,text=True)
  assert (r.returncode==0)==ok,(n,r.stdout,r.stderr)
 def digest(f):return hashlib.sha256(Path(f).read_bytes()).hexdigest()
 cases=json.loads((p/'cases.json').read_text());records=[];status=[]
 for i in range(1,13):
  for role in ['baseline','control','candidate']:
   for c in cases:
    dest=p/f'{i:02}-{role}'/c.replace('/','_');dest.mkdir(parents=True)
    target=c.endswith('/lin')and c.startswith(('compare/durable_update_','compare/durable_delete_','compare/durable_commit_'));v=80 if role=='candidate'and target else 100
    write(str(dest.relative_to(p)/'run.json'),dict(status='complete',cases=[dict(id=c)],observations=[dict(case=c,metric='wall',operations=1,value=v)]*8))
    dest.with_name(dest.name+'-resource.txt').write_text('Maximum resident set size (kbytes): 123\n')
    records.append(dict(round=i,mode=role,case=c,exit_code=0,peak_rss_kib=123,binary_sha256='c'if role=='candidate'else'b',glibc_tunables=None))
   status.append(dict(run=f'{i:02}-{role}',exit_code=0,subprocesses=63))
 write('process-status.json',records);write('status.json',status);write('test-status.json',dict(exit_code=0,source_restored=True,performance_qualified=False));write('mutant-status.json',dict(exit_code=101,expected_failure=True,runtime_assertion_caught=True));write('environment.json',dict(runtime_unchanged_all_roles=True,allocator_environment=dict(GLIBC_TUNABLES=None)))
 write('provenance.json',dict(source_restored=True,instrumentation=False,source_sha256={str(p/'compare.rs'):digest(p/'compare.rs')},original_compare_sha256=digest('benches/compare.rs'),compiled_compare_sha256=digest(p/'compare.rs'),candidate_persist_sha256=digest(p/'candidate-persist.rs'),binaries={'baseline':{'sha256':'b'},'candidate':{'sha256':'c'}}))
 run('summarize.py');run('verify.py');run('qualify.py')
 original=json.loads((p/'summary.json').read_text());altered=json.loads(json.dumps(original));altered[cases[0]]['series1']['gain_percent']=999;write('summary.json',altered);run('verify.py',False);write('summary.json',original)
 records[0]['peak_rss_kib']=124;write('process-status.json',records);run('verify.py',False);records[0]['peak_rss_kib']=123;write('process-status.json',records)
 write('process-status.json',records[:-1]);run('summarize.py',False);run('verify.py',False);write('process-status.json',records)
 case=cases[0];f=p/'01-candidate'/case.replace('/','_')/'run.json';original=f.read_text();d=json.loads(original);d['observations'][0]['value']=float('nan');f.write_text(json.dumps(d));run('summarize.py',False);run('verify.py',False);f.write_text(original)
 d=json.loads(original);d['observations']=[dict(case=case,metric='wall',operations=1,value=120)]*8;f.write_text(json.dumps(d));run('summarize.py');run('verify.py');run('qualify.py',False);f.write_text(original)
 control='compare/filter_eq/lin';f=p/'01-candidate'/control.replace('/','_')/'run.json';original=f.read_text();d=json.loads(original);d['observations']=[dict(case=control,metric='wall',operations=1,value=140)]*8
 # Regress every pair in one series so its median must fail the control gate.
 for i in range(1,7):
  target=p/f'{i:02}-candidate'/control.replace('/','_')/'run.json';target.write_text(json.dumps(d))
 run('summarize.py');run('verify.py');run('qualify.py',False)
 print('PASS: complete63-case matrix accepted; summary/RSS/missing-row/NaN corruption rejected; losing target pair and regressing control rejected by gate')
