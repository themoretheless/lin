from pathlib import Path
from safety_scan import scan_directory
import hashlib,json,math,os,platform,subprocess,sys
p=Path(__file__).resolve().parent;root=p.parents[2];sha=lambda b:hashlib.sha256(b).hexdigest();m=json.loads((p/'input.json').read_text())
for n,h in m['files_sha256'].items():assert sha((root/n).read_bytes())==h,n
assert platform.system()=='Linux'and platform.machine()=='x86_64'
assert not(p/'profile-status.json').exists(),'preserve earlier attempt'
up=json.loads(subprocess.check_output(['gh','run','view',m['binary_run'],'--json','status,conclusion,headSha,jobs'],cwd=root,text=True));assert up['status']=='completed'and up['headSha']==m['binary_head']
steps={s['name']:s['conclusion']for j in up['jobs']for s in j['steps']}
for name in ['Audit all observations values and original processes','Verify final frozen inputs without overlay or source mutation']:assert steps[name]=='success'
folder=Path('/tmp/lin-current-root-profile-executable');assert not folder.exists();folder.mkdir();subprocess.run(['gh','run','download',m['binary_run'],'--name','cow-extent-public-peers-exact-linux-executable','--dir',str(folder)],check=True)
files=[f for f in folder.rglob('*')if f.is_file()];assert len(files)==1;binary=files[0];b=binary.read_bytes();assert sha(b)==m['binary_sha256']and len(b)==m['binary_bytes']and b[:6]==b'\x7fELF\x02\x01'and int.from_bytes(b[18:20],'little')==62;binary.chmod(0o755)
profile_env=dict(m['profiled_environment']);assert set(profile_env)=={'HOME','PATH','LANG','LC_ALL'}
profile_home=Path(profile_env['HOME']);assert not profile_home.exists();profile_home.mkdir(mode=0o700)
tools=sorted(f for f in Path('/usr/lib/linux-tools').glob('*/perf')if f.is_file()and os.access(f,os.X_OK));assert tools,'Installed perf executable missing';perf=tools[-1]
environment=dict(platform=platform.platform(),uname=subprocess.check_output(['uname','-a'],text=True,env=profile_env).strip(),perf_path=str(perf),perf_version=subprocess.check_output([str(perf),'--version'],text=True,env=profile_env).strip(),perf_event_paranoid=Path('/proc/sys/kernel/perf_event_paranoid').read_text().strip(),binary_sha256=sha(b),binary_source_run=m['binary_run'],binary_source_head=m['binary_head'],runtime_reference=m['runtime_reference'],features=m['features'],rebuilt=False,frame_pointer_flags_added=False,scope='Whole unmodified worker including preparation, validation, teardown and output; not operation latency proof')
environment['profiled_environment_keys']=sorted(profile_env)
(p/'environment.json').write_text(json.dumps(environment,indent=2)+'\n');request=dict(insert_rows=100000,samples=48,timestamp_ms=1700000000000);records=[]
for i in [1,2,3]:
 data=p/f'profile-{i}.data';response=p/f'worker-{i}.json';log=p/f'perf-{i}.log';assert not any(f.exists()for f in [data,response,log])
 command=[str(perf),'record','--event','cpu-clock:u','--freq','99','--call-graph','dwarf,16384','--output',str(data),'--',str(binary)]
 with response.open('w')as out,log.open('w')as err:r=subprocess.run(command,cwd=root,input=json.dumps(request)+'\n',text=True,stdout=out,stderr=err,env=profile_env)
 assert r.returncode==0 and data.stat().st_size>0
 d=json.loads(response.read_text());assert len(d['samples_ns'])==48 and all(type(x)in(int,float)and math.isfinite(x)and x>0 for x in d['samples_ns'])and len(d['result'])==100000 and d['statements']==10 and d['statement_rows_limit']==10000
 for row_index,row in enumerate(d['result']):assert row==[f'd-{row_index}',f'bench://{row_index}','rag'if row_index%2==0 else'sys',f'doc {row_index} wal note'if row_index%10==0 else f'doc {row_index} plain',1700000000000,f'body {row_index}']
 for label,args in [('self-report',['report','--stdio','--no-children','--sort','symbol,dso','--percent-limit','0.1']),('stacks',['script']),('buildids',['buildid-list'])]:
  cmd=[str(perf),*args,'--input',str(data)]
  with(p/f'{label}-{i}.txt').open('w')as out,(p/f'{label}-{i}.log').open('w')as err:r=subprocess.run(cmd,cwd=root,stdout=out,stderr=err,env=profile_env)
  assert r.returncode==0,(i,label)
 records.append(dict(process=i,command=command,request=request,exit=0,profile_sha256=sha(data.read_bytes()),worker_response_sha256=sha(response.read_bytes()),independent_final_rows_verified=100000,internal_worker_samples=48));(p/'process-status.json').write_text(json.dumps(records,indent=2)+'\n')
for n,h in m['files_sha256'].items():assert sha((root/n).read_bytes())==h,n
assert sha(binary.read_bytes())==m['binary_sha256']
scan=scan_directory(p);assert all(v==0 for values in scan.values()for v in values.values()),'Credential-pattern scan failed; no public artifact upload'
(p/'safety-scan.json').write_text(json.dumps(dict(complete=True,counts=scan,profiled_environment_keys=sorted(profile_env)),indent=2)+'\n')
(p/'profile-status.json').write_text(json.dumps(dict(complete=True,processes=3,samples=144,independent_final_six_field_rows_verified=300000,internal_worker_validated_rows=14400000,source_and_binary_unchanged=True,profiled_exact_public_executable=True,performance_qualified=False,peer_comparisons=0,public_artifacts_safe=True),indent=2)+'\n');print('Exact unmodified current worker profiles complete; no performance or peer qualification')
