from pathlib import Path
import hashlib,json,os,subprocess,sys,tempfile
p=Path(__file__).resolve().parent;root=p.parents[2]
sys.path.insert(0,str(root/'scripts'))
from benchmark_role_balance import schedule
roles=['append_a','append_b','fixed_sparse_a','fixed_sparse_b','fixed_zeroed_a','fixed_zeroed_b']
results=[]
for corruption in [None,'latency','command','binary','readback','source']:
 with tempfile.TemporaryDirectory(prefix='lin-eof-audit-fixture-')as tmp:
  q=Path(tmp).resolve()/'a/b/c';q.mkdir(parents=True);(q/'raw').mkdir();(q/'audit.py').write_bytes((p/'audit.py').read_bytes())
  exe=q/'probe';exe.write_bytes(b'fixture executable');h=hashlib.sha256(exe.read_bytes()).hexdigest()
  def put(name,d):(q/name).write_text(json.dumps(d))
  put('input.json',dict(files_sha256={}));put('environment.json',dict(binary=dict(path=str(exe),sha256=h)))
  put('status.json',dict(complete=True,processes=36,observations=2304,source_unchanged=True,owned_files_removed=True,performance_qualification=False))
  records=[]
  for item in schedule([dict(id='point_sync')],roles,rounds=6):
   mode=item['role'].rsplit('_',1)[0];name=f"raw/{item['round']:02}-{item['role']}.json";v={'append':100,'fixed_sparse':80,'fixed_zeroed':60}[mode]+(2 if item['role'].endswith('_b')else 0)
   put(name,dict(mode=mode,samples=64,payload_bytes=4125,logical_end=280384,physical_bytes=280384 if mode=='append'else 8388608,full_readback_verified=True,owned_file_removed=True,latencies_ns=[v]*64))
   records.append(dict(**item,command=[str(exe),mode,str(q/'owned-files'/f"{item['round']:02}-{item['role']}.bin")],exit_code=0,binary_sha256=h,raw=name,median_ns=v))
  if corruption in ['latency','readback']:
   f=q/records[0]['raw'];d=json.loads(f.read_text())
   if corruption=='latency':d['latencies_ns'][0]=0
   else:d['full_readback_verified']=False
   f.write_text(json.dumps(d))
  elif corruption=='command':records[0]['command'][1]='wrong_mode'
  elif corruption=='binary':exe.write_bytes(b'replaced executable')
  elif corruption=='source':put('input.json',dict(files_sha256={'absent-input.rs':'incorrect'}))
  put('process-status.json',records)
  env=dict(os.environ,PYTHONPATH=str(root/'scripts'))
  run=subprocess.run([sys.executable,str(q/'audit.py')],env=env,capture_output=True,text=True)
  assert (run.returncode==0)==(corruption is None),(corruption,run.stdout,run.stderr)
  if corruption is None:
   audit=json.loads((q/'audit.json').read_text());assert audit['summary']['fixed_zeroed']['gain_percent']==40 and audit['observations']==2304
  results.append(dict(corruption=corruption,exit_code=run.returncode,expected_rejection=corruption is not None))
(p/'audit-tests.json').write_text(json.dumps(dict(fixture_tests=6,all_passed=True,tests=results,actual_sync_measurements=0),indent=2)+'\n')
print('PASS valid 2304-observation fixture and five corruption rejections')
