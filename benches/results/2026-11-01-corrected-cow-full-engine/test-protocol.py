from pathlib import Path
import tempfile,json,shutil,subprocess,hashlib
src=Path(__file__).resolve().parent
with tempfile.TemporaryDirectory(prefix='lin-wal-reserve-protocol-')as tmp:
 p=Path(tmp).resolve()
 for n in ['summarize.py','verify.py','qualify.py','cases.json','source-inputs.json','compare.rs']:shutil.copy2(src/n,p/n)
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
    target=c.endswith('/lin')and (c.startswith('compare/update_1row_')or c in ['compare/wal_scalar_full_update/lin','compare/wal_scalar_normal_update/lin']);v=80 if role=='candidate'and target else 100
    write(str(dest.relative_to(p)/'run.json'),dict(status='complete',cases=[dict(id=c)],observations=[dict(case=c,metric='wall',operations=1,value=v)]*8))
    dest.with_name(dest.name+'-resource.txt').write_text('Maximum resident set size (kbytes): 123\n')
    records.append(dict(round=i,mode=role,case=c,exit_code=0,peak_rss_kib=123,binary_sha256='c'if role=='candidate'else'b',glibc_tunables=None))
   status.append(dict(run=f'{i:02}-{role}',exit_code=0,subprocesses=67))
 write('process-status.json',records);write('status.json',status);write('test-status.json',dict(exit_code=0,source_restored=True,performance_qualified=False));write('mutant-status.json',dict(exit_code=101,expected_failure=True,runtime_assertion_caught=True));write('environment.json',dict(runtime_unchanged_all_roles=True,allocator_environment=dict(GLIBC_TUNABLES=None)))
 frozen=json.loads((p/'source-inputs.json').read_text());binaries={}
 for role in ['baseline','candidate']:
  binary=p/(role+'-binary');binary.write_bytes(role.encode());binaries[role]=dict(path=str(binary),sha256=digest(binary),cargo_target_dir='target/corrected-full-engine-'+role,source_sha256=frozen[role]['source_sha256'])
 for record in records:record['binary_sha256']=binaries['baseline'if record['mode']=='control'else record['mode']]['sha256']
 by_key={(r['round'],r['mode'],r['case']):r for r in records};ordered=[]
 for round in range(1,13):
  rotated=cases[round%67:]+cases[:round%67]
  for index,c in enumerate(rotated):
   roles=['baseline','control','candidate'];shift=(round+index)%3;roles=roles[shift:]+roles[:shift]
   for role in roles:
    r=by_key[round,role,c];base=p/f'{round:02}-{role}';name=c.replace('/','_')
    binary=binaries['baseline'if role=='control'else role]['path']
    r['command']=['/usr/bin/time','-v','-o',str(base/(name+'-resource.txt')),'--',binary,'--filter',c,'--samples','8','--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output',str(base/name)]
    f=base/name/'run.json';d=json.loads(f.read_text());d.update(schema=1,id=f'{round}-{role}-{c}',environment=dict(arch='x86_64',os='linux'),provenance=dict(samples='8',warmup_ns='0'));write(str(f.relative_to(p)),d);ordered.append(r)
 records=ordered
 write('process-status.json',records)
 write('provenance.json',dict(source_restored=True,instrumentation=False,source_sha256=frozen['candidate']['source_sha256'],original_compare_sha256=digest('benches/compare.rs'),compiled_compare_sha256=digest(p/'compare.rs'),frozen_commits={r:frozen[r]['commit']for r in binaries},binaries=binaries))
 smokes=[]
 for role in binaries:
  for c in cases:
   dest=p/'local-smoke'/role/c.replace('/','_');dest.mkdir(parents=True)
   write(str(dest.relative_to(p)/'run.json'),dict(status='complete',cases=[dict(id=c)],observations=[dict(case=c,metric='wall',operations=1,value=100)]))
   smokes.append(dict(role=role,case=c,exit_code=0))
 write('local-smoke-status.json',dict(correctness_smoke_only=True,performance_qualified=False,runs=smokes))
 run('summarize.py');run('verify.py');run('qualify.py')
 original_meta=(p/'provenance.json').read_text()
 bad=json.loads(original_meta);bad['frozen_commits']['baseline']='0'*40;write('provenance.json',bad);run('verify.py',False);(p/'provenance.json').write_text(original_meta)
 bad=json.loads(original_meta);bad['binaries']['candidate']['cargo_target_dir']='target/corrected-full-engine-baseline';write('provenance.json',bad);run('verify.py',False);(p/'provenance.json').write_text(original_meta)
 original_smokes=(p/'local-smoke-status.json').read_text();bad=json.loads(original_smokes);bad['runs'].pop();write('local-smoke-status.json',bad);run('verify.py',False);(p/'local-smoke-status.json').write_text(original_smokes)
 records[0],records[1]=records[1],records[0];write('process-status.json',records);run('verify.py',False);records[0],records[1]=records[1],records[0];write('process-status.json',records)
 original_command=records[0]['command'][:];records[0]['command'][records[0]['command'].index('--warmup-ms')+1]='1';write('process-status.json',records);run('verify.py',False);records[0]['command']=original_command;write('process-status.json',records)
 original=json.loads((p/'summary.json').read_text());altered=json.loads(json.dumps(original));altered[cases[0]]['series1']['gain_percent']=999;write('summary.json',altered);run('verify.py',False);write('summary.json',original)
 records[0]['peak_rss_kib']=124;write('process-status.json',records);run('verify.py',False);records[0]['peak_rss_kib']=123;write('process-status.json',records)
 write('process-status.json',records[:-1]);run('summarize.py',False);run('verify.py',False);write('process-status.json',records)
 case='compare/update_1row_1k/lin';f=p/'01-candidate'/case.replace('/','_')/'run.json';original=f.read_text();d=json.loads(original);d['observations'][0]['value']=float('nan');f.write_text(json.dumps(d));run('summarize.py',False);run('verify.py',False);f.write_text(original)
 d=json.loads(original);d['observations']=[dict(case=case,metric='wall',operations=1,value=120)]*8;f.write_text(json.dumps(d));run('summarize.py');run('verify.py');run('qualify.py',False);f.write_text(original)
 control='compare/filter_eq/lin';f=p/'01-candidate'/control.replace('/','_')/'run.json';original=f.read_text();d=json.loads(original);d['observations']=[dict(case=control,metric='wall',operations=1,value=140)]*8
 # Regress every pair in one series so its median must fail the control gate.
 for i in range(1,7):
  target=p/f'{i:02}-candidate'/control.replace('/','_')/'run.json';row=json.loads(target.read_text());row['observations']=d['observations'];target.write_text(json.dumps(row))
 run('summarize.py');run('verify.py');run('qualify.py',False)
 print('PASS: complete67-case matrix accepted; summary/RSS/missing-row/NaN corruption rejected; losing target pair and regressing control rejected by gate')
