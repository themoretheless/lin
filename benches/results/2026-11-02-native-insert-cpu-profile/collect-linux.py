from pathlib import Path
import hashlib,json,math,statistics,subprocess,tarfile
p=Path(__file__).resolve().parent;root=p.parents[2]
run='41396688130';head='15da4231ce152dcf506025cd9e7f6c9e98f3976f'
sha=lambda b:hashlib.sha256(b).hexdigest()
meta=json.loads(subprocess.check_output(['gh','run','view',run,'--json','status,conclusion,headSha,url'],cwd=root,text=True))
assert meta['status']=='completed' and meta['conclusion']=='success' and meta['headSha']==head,meta
raw=Path('/tmp/lin-native-insert-profile-'+run);exe=Path('/tmp/lin-native-insert-profile-executable-'+run)
out=root/'benches/results/2026-11-02-native-insert-cpu-profile-linux'
assert not raw.exists() and not exe.exists() and not out.exists(),'preserve previous attempt; revalidate rather than overwrite'
raw.mkdir();exe.mkdir()
for name,dest in [('native-insert-cpu-profile',raw),('native-profile-exact-linux-executable',exe)]:
 subprocess.run(['gh','run','download',run,'-n',name,'-D',str(dest)],cwd=root,check=True)
inputs=json.loads((raw/'input.json').read_text())
assert (raw/'input.json').read_bytes()==subprocess.check_output(['git','show',head+':'+str(p.relative_to(root))+'/input.json'],cwd=root)
for name,h in inputs['files_sha256'].items():
 assert sha(subprocess.check_output(['git','show',head+':'+name],cwd=root))==h,name
 local=raw/Path(name).name
 if local.is_file():assert sha(local.read_bytes())==h,name
for name,h in inputs['runtime_sha256'].items():assert sha(subprocess.check_output(['git','show',head+':'+name],cwd=root))==h,name
status=json.loads((raw/'status.json').read_text());env=json.loads((raw/'environment.json').read_text());summary=json.loads((raw/'summary.json').read_text())
assert status==dict(complete=True,native_insert_rows=100000,exact_field_readbacks=200000,measured_samples=24,worker_test_passed=True,perf_recorded=True,source_unchanged=True,performance_qualified=False)
assert env['perf_readiness_exit']==0 and env['source_unchanged'] is True and env['runtime_reference']==inputs['runtime_reference']
assert '1.96.0' in env['rustc'] and 'x86_64-unknown-linux-gnu' in env['rustc']
assert env['build_env']['RUSTFLAGS']=='-C force-frame-pointers=yes -C debuginfo=1'
assert env['commands']==[['cargo','test','--release','-p','lin-peer-worker'],['cargo','build','--release','-p','lin-peer-worker','--message-format=json-render-diagnostics']]
assert summary['profiling_only'] is True and summary['performance_qualified'] is False and summary['not_peer_superiority_evidence'] is True
binary=exe/'lin-native-insert-profile';assert sha(binary.read_bytes())==env['binary']['sha256'];assert binary.read_bytes()[:4]==b'\x7fELF'
request=json.loads((raw/'request.json').read_text());assert request==dict(insert_rows=100000,samples=12,timestamp_ms=1700000000000)
for mode in ['unprofiled','profiled']:
 d=json.loads((raw/(mode+'-response.json')).read_text());values=d['samples_ns'];assert len(values)==12 and all(type(v)in(int,float)and math.isfinite(v)and v>0 for v in values)
 assert len(d['result'])==100000 and d['statements']==10 and d['statement_rows_limit']==10000
 for i,row in enumerate(d['result']):assert row==[f'd-{i}',f'bench://{i}','rag'if i%2==0 else'sys',f'doc {i} wal note'if i%10==0 else f'doc {i} plain',1700000000000,f'body {i}'],(mode,i)
 s=summary['measurements'][mode];assert s['samples_ns']==values and s['median_ns']==statistics.median(values) and s['all_six_fields_verified_rows']==100000
 command=[env['binary']['path']]if mode=='unprofiled'else[env['perf_path'],'record','-e','cpu-clock:u','-F','499','--call-graph','fp','-o',s['command'][-3],'--',env['binary']['path']]
 assert s['command']==command,(mode,s['command'])
assert summary['profiled_over_unprofiled']==summary['measurements']['profiled']['median_ns']/summary['measurements']['unprofiled']['median_ns']
for name in ['perf.data','perf-report.txt','perf-script.txt']:assert(raw/name).stat().st_size>0,name
manifest={str(f.relative_to(raw)):dict(size=f.stat().st_size,sha256=sha(f.read_bytes()))for f in sorted(raw.rglob('*'))if f.is_file()}
out.mkdir()
with tarfile.open(out/'raw.tar.gz','w:gz')as archive:
 for name in manifest:archive.add(raw/name,arcname=name,recursive=False)
(out/'raw-manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
for name,data in [('run-metadata.json',meta),('summary.json',summary),('environment.json',env),('independent-audit.json',dict(all_fields_verified_rows=200000,samples=24,source_and_binary_verified=True,performance_qualified=False,peer_comparisons=0))]:(out/name).write_text(json.dumps(data,indent=2)+'\n')
(out/'perf-report.txt').write_bytes((raw/'perf-report.txt').read_bytes())
print(json.dumps(dict(archived_files=len(manifest),rows_verified=200000,peer_comparisons=0,performance_qualified=False)))
