from pathlib import Path
import hashlib,json,math,os,platform,shutil,statistics,subprocess
p=Path(__file__).resolve().parent;root=p.parents[2];sha=lambda b:hashlib.sha256(b).hexdigest();inputs=json.loads((p/'input.json').read_text())
assert platform.system()=='Linux'
for name,h in inputs['files_sha256'].items():assert sha((root/name).read_bytes())==h,name
for name,h in inputs['runtime_sha256'].items():assert sha((root/name).read_bytes())==h,name
assert not(p/'status.json').exists(),'preserve prior attempt'
perf_candidates=[f for f in Path('/usr/lib/linux-tools').glob('*/perf')if f.is_file()]
assert perf_candidates,'Linux perf tools unavailable'
perf=max(perf_candidates,key=lambda f:f.stat().st_mtime)
metadata=dict(platform=platform.platform(),rustc=subprocess.check_output(['rustc','-Vv'],text=True),perf_path=str(perf),perf_version=subprocess.check_output([str(perf),'--version'],text=True),runtime_reference=inputs['runtime_reference'],performance_qualified=False,profiling_only=True)
with(p/'perf-readiness.log').open('w')as log:r=subprocess.run([str(perf),'stat','-e','cpu-clock:u','--','true'],stdout=log,stderr=subprocess.STDOUT)
metadata['perf_readiness_exit']=r.returncode;(p/'environment.json').write_text(json.dumps(metadata,indent=2)+'\n');assert r.returncode==0,'preserve unavailable perf result; no fabricated profile'
env=dict(os.environ,CARGO_BUILD_JOBS='2',CARGO_TARGET_DIR=str(root/'target/native-insert-profile'),RUSTFLAGS='-C force-frame-pointers=yes -C debuginfo=1')
assert not any(k.startswith('CARGO_PROFILE_')for k in os.environ)
metadata['build_env']={k:env[k]for k in ['CARGO_BUILD_JOBS','CARGO_TARGET_DIR','RUSTFLAGS']}
qa=['cargo','test','--release','-p','lin-peer-worker'];build=['cargo','build','--release','-p','lin-peer-worker','--message-format=json-render-diagnostics'];metadata['commands']=[qa,build]
with(p/'worker-tests.log').open('w')as log:r=subprocess.run(qa,cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT)
assert r.returncode==0
with(p/'build.log').open('w')as log:r=subprocess.run(build,cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT)
assert r.returncode==0
artifacts=[]
for line in(p/'build.log').read_text().splitlines():
 try:d=json.loads(line)
 except ValueError:continue
 if d.get('reason')=='compiler-artifact'and d.get('target',{}).get('name')=='lin-peer-worker'and d.get('executable'):artifacts.append(d['executable'])
assert len(artifacts)==1
binary=Path('/tmp/lin-native-insert-profile');assert not binary.exists();shutil.copy2(artifacts[0],binary);metadata['binary']=dict(path=str(binary),sha256=sha(binary.read_bytes()))
request=dict(insert_rows=100000,samples=12,timestamp_ms=1700000000000);(p/'request.json').write_text(json.dumps(request)+'\n');summaries={}
for mode in ['unprofiled','profiled']:
 command=[str(binary)]if mode=='unprofiled'else[str(perf),'record','-e','cpu-clock:u','-F','499','--call-graph','fp','-o',str(p/'perf.data'),'--',str(binary)]
 with(p/'request.json').open('r')as stdin,(p/(mode+'-response.json')).open('w')as stdout,(p/(mode+'.log')).open('w')as log:r=subprocess.run(command,cwd=root,env=env,stdin=stdin,stdout=stdout,stderr=log)
 assert r.returncode==0,(mode,r.returncode)
 d=json.loads((p/(mode+'-response.json')).read_text());values=d['samples_ns'];assert len(values)==12 and all(type(v)in(int,float)and math.isfinite(v)and v>0 for v in values)
 assert d['statements']==10 and d['statement_rows_limit']==10000 and len(d['result'])==100000
 for i,row in enumerate(d['result']):assert row==[f'd-{i}',f'bench://{i}','rag'if i%2==0 else'sys',f'doc {i} wal note'if i%10==0 else f'doc {i} plain',request['timestamp_ms'],f'body {i}'],(mode,i)
 summaries[mode]=dict(command=command,median_ns=statistics.median(values),samples_ns=values,all_six_fields_verified_rows=100000,statements=10)
for name,args in [('perf-report.txt',['report','--stdio','--children','--percent-limit','0.5']),('perf-script.txt',['script'])]:
 with(p/name).open('w')as output,(p/(name+'.log')).open('w')as log:r=subprocess.run([str(perf),*args,'-i',str(p/'perf.data')],stdout=output,stderr=log)
 assert r.returncode==0,name
assert(p/'perf.data').stat().st_size>0 and(p/'perf-script.txt').stat().st_size>0
for name,h in inputs['files_sha256'].items():assert sha((root/name).read_bytes())==h,name
for name,h in inputs['runtime_sha256'].items():assert sha((root/name).read_bytes())==h,name
assert sha(binary.read_bytes())==metadata['binary']['sha256']
metadata['source_unchanged']=True;(p/'environment.json').write_text(json.dumps(metadata,indent=2)+'\n')
(p/'summary.json').write_text(json.dumps(dict(profiling_only=True,performance_qualified=False,not_peer_superiority_evidence=True,profiled_over_unprofiled=summaries['profiled']['median_ns']/summaries['unprofiled']['median_ns'],measurements=summaries),indent=2)+'\n')
(p/'status.json').write_text(json.dumps(dict(complete=True,native_insert_rows=100000,exact_field_readbacks=200000,measured_samples=24,worker_test_passed=True,perf_recorded=True,source_unchanged=True,performance_qualified=False),indent=2)+'\n')
print('Complete native insertion CPU profile; do not treat instrumented timings as peer acceptance')
