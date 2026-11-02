from pathlib import Path
import hashlib,json,os,platform,statistics,subprocess,sys
p=Path(__file__).resolve().parent;root=p.parents[2]
sys.path.insert(0,str(root/'scripts'))
from benchmark_role_balance import schedule
sha=lambda data:hashlib.sha256(data).hexdigest()
inputs=json.loads((p/'input.json').read_text())
for name,h in inputs['files_sha256'].items():assert sha((root/name).read_bytes())==h,name
assert platform.system()=='Linux'
assert not(p/'process-status.json').exists(),'retain previous attempt'
exe=Path('/tmp/lin-fixed-eof-sync-linux');assert not exe.exists()
command=['rustc','--edition=2024','-O',str(p/'probe.rs'),'-o',str(exe)]
with(p/'build.log').open('w')as log:subprocess.run(command,cwd=root,stdout=log,stderr=subprocess.STDOUT,check=True)
binary_hash=sha(exe.read_bytes())
metadata=dict(command=command,binary=dict(path=str(exe),sha256=binary_hash),rustc=subprocess.check_output(['rustc','-Vv'],text=True),platform=platform.platform(),kernel=platform.release(),filesystem=subprocess.check_output(['df','-T',str(p)],text=True),performance_qualification=False)
(p/'environment.json').write_text(json.dumps(metadata,indent=2)+'\n')
roles=['append_a','append_b','fixed_sparse_a','fixed_sparse_b','fixed_zeroed_a','fixed_zeroed_b']
plan=schedule([dict(id='point_sync')],roles,rounds=6)
files=p/'owned-files';files.mkdir(exist_ok=False)
raw=p/'raw';raw.mkdir(exist_ok=False);records=[]
for item in plan:
 mode=item['role'].rsplit('_',1)[0];target=files/f"{item['round']:02}-{item['role']}.bin";dest=raw/f"{item['round']:02}-{item['role']}.json"
 assert sha(exe.read_bytes())==binary_hash
 command=[str(exe),mode,str(target)]
 with dest.open('w')as output, dest.with_suffix('.log').open('w')as log:run=subprocess.run(command,stdout=output,stderr=log)
 record=dict(**item,command=command,exit_code=run.returncode,binary_sha256=binary_hash,raw=str(dest.relative_to(p)))
 assert run.returncode==0,record
 d=json.loads(dest.read_text());assert d['mode']==mode and d['samples']==64 and len(d['latencies_ns'])==64
 assert d['payload_bytes']==4125 and d['logical_end']==16384+64*4125
 assert d['physical_bytes']==(d['logical_end']if mode=='append'else 8388608)
 assert d['full_readback_verified']and d['owned_file_removed']and not target.exists()
 assert all(type(v)is int and v>0 for v in d['latencies_ns'])
 record['median_ns']=statistics.median(d['latencies_ns']);records.append(record)
 (p/'process-status.json').write_text(json.dumps(records,indent=2)+'\n');print(item['round'],item['role'],record['median_ns'],flush=True)
for name,h in inputs['files_sha256'].items():assert sha((root/name).read_bytes())==h,name
assert not list(files.iterdir())
(p/'status.json').write_text(json.dumps(dict(complete=True,processes=36,observations=2304,source_unchanged=True,owned_files_removed=True,performance_qualification=False),indent=2)+'\n')
