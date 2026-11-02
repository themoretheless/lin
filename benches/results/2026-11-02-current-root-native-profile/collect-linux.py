from pathlib import Path
from safety_scan import scan_directory
from collections import Counter
import argparse,hashlib,json,math,re,subprocess,tarfile
p=Path(__file__).resolve().parent;root=p.parents[2];sha=lambda b:hashlib.sha256(b).hexdigest();parser=argparse.ArgumentParser();parser.add_argument('--run',required=True);parser.add_argument('--head',required=True);args=parser.parse_args();run=args.run;head=args.head;assert run.isdecimal()and len(head)==40 and run!='41425364783','Original unsafe raw profile stays private';raw=Path('/tmp/lin-current-root-native-profile-'+run);out=root/'benches/results/2026-11-02-current-root-native-profile-linux'
meta=json.loads(subprocess.check_output(['gh','run','view',run,'--json','status,conclusion,headSha,url'],cwd=root,text=True));assert meta['status']=='completed'and meta['conclusion']=='success'and meta['headSha']==head
assert raw.is_dir()and not out.exists(),'Keep downloaded evidence and never overwrite collection'
assert raw.stat().st_mode&0o777==0o700,'Keep downloads owner-only until independently scanned'
independent_scan=scan_directory(raw);assert all(v==0 for counts in independent_scan.values()for v in counts.values()),'Raw credential-pattern scan failed; do not publish'
load=lambda n:json.loads((raw/n).read_text());inputs=load('input.json');assert(raw/'input.json').read_bytes()==subprocess.check_output(['git','show',head+':'+str(p.relative_to(root))+'/input.json'],cwd=root)
for n,h in inputs['files_sha256'].items():
 assert sha(subprocess.check_output(['git','show',head+':'+n],cwd=root))==h,n
 if n.startswith(str(p.relative_to(root))+'/'):assert sha((raw/Path(n).name).read_bytes())==h,n
status=load('profile-status.json');assert status==dict(complete=True,processes=3,samples=144,independent_final_six_field_rows_verified=300000,internal_worker_validated_rows=14400000,source_and_binary_unchanged=True,profiled_exact_public_executable=True,performance_qualified=False,peer_comparisons=0,public_artifacts_safe=True)
safety=load('safety-scan.json');assert safety['complete']and all(v==0 for values in safety['counts'].values()for v in values.values())
env=load('environment.json');assert env['binary_sha256']==inputs['binary_sha256']=='4a4c3918ed1064c49e9b80fb0512f6afedb2e4c6d239031a7039a6766c761df9'and env['runtime_reference']==inputs['runtime_reference']=='60e64559ec0e5a21efd490d6da5a744e3ac8d0e2'and env['rebuilt']is False and env['frame_pointer_flags_added']is False
assert env['features']==inputs['features']==['default','experimental-extent-wal']and env['binary_source_run']=='41413513001'and env['binary_source_head']=='8445e1e198e6020ec85c49d991650713dce8e251'
assert set(env['profiled_environment_keys'])=={'HOME','PATH','LANG','LC_ALL'}
records=load('process-status.json');assert len(records)==3;scoped=[];rows=0
for i,record in enumerate(records,1):
 assert record['process']==i and record['exit']==0 and record['request']==dict(insert_rows=100000,samples=48,timestamp_ms=1700000000000)
 assert record['command']==[env['perf_path'],'record','--event','cpu-clock:u','--freq','99','--call-graph','dwarf,16384','--output',f'/home/runner/work/lin/lin/{p.relative_to(root)}/profile-{i}.data','--','/tmp/lin-current-root-profile-executable/lin-cow-extent-peer-built']
 assert sha((raw/f'profile-{i}.data').read_bytes())==record['profile_sha256']and sha((raw/f'worker-{i}.json').read_bytes())==record['worker_response_sha256']
 worker=load(f'worker-{i}.json');assert len(worker['samples_ns'])==48 and all(type(x)in(int,float)and math.isfinite(x)and x>0 for x in worker['samples_ns'])and worker['statements']==10 and worker['statement_rows_limit']==10000 and len(worker['result'])==100000
 for j,row in enumerate(worker['result']):assert row==[f'd-{j}',f'bench://{j}','rag'if j%2==0 else'sys',f'doc {j} wal note'if j%10==0 else f'doc {j} plain',1700000000000,f'body {j}'];rows+=1
 report=(raw/f'self-report-{i}.txt').read_text();assert '# Total Lost Samples: 0'in report
 blocks=(raw/f'stacks-{i}.txt').read_text().strip().split('\n\n');capture=re.search(r'\((\d+) samples\)',(raw/f'perf-{i}.log').read_text());assert capture and int(capture[1])==len(blocks)
 total_weight=0;tx_weight=0;tx_samples=0;worker_weight=0;short=0;unknown=0;stages=Counter();leaves=Counter();frames_counter=Counter()
 for block in blocks:
  lines=block.splitlines();period=re.search(r':\s+(\d+)\s+cpu-clock:u:',lines[0]);assert period and int(period[1])>0;weight=int(period[1]);total_weight+=weight;frames=[]
  for line in lines[1:]:
   frame=re.match(r'\s*[0-9a-f]+\s+(.+?)\s+\((.+)\)\s*$',line)
   if frame:frames.append((re.sub(r'\+0x[0-9a-f]+$','',frame[1]),frame[2]))
  assert frames;short+=len(frames)<3;unknown+=frames[0][0]=='[unknown]';names=[f[0]for f in frames]
  inside=any('Transaction'in f and('run_prepared'in f or 'commit'in f)for f in names)
  if not inside:
   if any('append_batch_shape_rows'in f for f in names):worker_weight+=weight
   continue
  tx_samples+=1;tx_weight+=weight;leaves[frames[0]]+=weight;frames_counter.update(set(names))
  categories=[('row_maps','row_maps_register_slab'),('embedding','lin::embed::'),('secondary_indexes','lin::index::'),('fts','FtsIndex'),('row_build','append_batch_shape_rows')]
  category=next((label for label,needle in categories if any(needle in f for f in names)),'other');stages[category]+=weight
 assert tx_samples>500 and short<len(blocks)//20 and sum(stages.values())==tx_weight
 scoped.append(dict(process=i,cpu_clock_samples=len(blocks),cpu_period_weight=total_weight,transaction_ancestor_samples=tx_samples,transaction_ancestor_cpu_percent=100*tx_weight/total_weight,known_offthread_row_builder_cpu_percent=100*worker_weight/total_weight,lost_samples=0,short_stacks=short,unresolved_leaf_samples=unknown,transaction_stage_cpu_percent={k:100*v/tx_weight for k,v in stages.items()},transaction_leaf_top=[dict(symbol=k[0],dso=k[1],scoped_cpu_percent=100*v/tx_weight)for k,v in leaves.most_common(25)],transaction_frames_top=frames_counter.most_common(20)))
assert rows==300000
manifest={str(f.relative_to(raw)):dict(size=f.stat().st_size,sha256=sha(f.read_bytes()))for f in sorted(raw.rglob('*'))if f.is_file()};out.mkdir();archive=out/'raw.tar.gz'
with tarfile.open(archive,'w:gz')as tar:
 for n in manifest:tar.add(raw/n,arcname=n,recursive=False)
with tarfile.open(archive)as tar:assert {m.name:dict(size=m.size,sha256=sha(tar.extractfile(m).read()))for m in tar.getmembers()}==manifest
for name,data in [('raw-manifest.json',manifest),('run-metadata.json',meta),('scoped-profile.json',scoped),('independent-audit.json',dict(raw_files=len(manifest),all_member_hashes_verified=True,source_git_blobs_verified=True,exact_public_binary_sha256=inputs['binary_sha256'],worker_processes=3,worker_samples=144,independent_final_rows_verified=300000,cpu_clock_samples=sum(x['cpu_clock_samples']for x in scoped),lost_samples=0,archive_sha256=sha(archive.read_bytes()),performance_qualified=False,peer_comparisons=0))]:(out/name).write_text(json.dumps(data,indent=2)+'\n')
lines=['# Exact current root native worker: CPU sampling diagnostic','','Unmodified public ELF4a4c3918e, runtime60e64559e, default+extent, original Rust1.96.0 build. No new compiler flags or source instrumentation. Three100k/48-sample processes;300000final six-field rows independently checked,14.4million rows internally checked by the unchanged worker. All raw profiles, stack traces, requests, reports and SHA manifests retained. Zero lost CPU samples.','','Scope below selects stacks containing Transaction::run_prepared or Transaction::commit. It excludes much preparation/validation/teardown. This is a conservative call-chain subset, not exact operation-timer boundaries: some worker-thread construction and Handle drops have no transaction ancestor. Known offthread row-builder samples are reported separately in JSON. Unknown libc leaf symbols remain in caller attribution; incomplete symbol names do not become omitted samples. No elapsed-time gain, CPU/wall conversion or external peer win follows.','','| Process | CPU samples | TX ancestor samples | TX share of whole profile % | Embedding within TX % | FTS within TX % | Index within TX % | Maps within TX % |','|---|---:|---:|---:|---:|---:|---:|---:|']
for d in scoped:
 s=d['transaction_stage_cpu_percent'];lines.append(f"| {d['process']} | {d['cpu_clock_samples']} | {d['transaction_ancestor_samples']} | {d['transaction_ancestor_cpu_percent']:.2f} | {s.get('embedding',0):.2f} | {s.get('fts',0):.2f} | {s.get('secondary_indexes',0):.2f} | {s.get('row_maps',0):.2f} |")
lines+=['','Preparation is a major part of the whole profile; optimizing parser self time would not directly target the measured insertion clock. Within the selected transaction stacks, embedding, FTS, secondary indexes and row maps provide consistent next investigation areas. Instruction-level or allocation hypotheses still require their own source-bound correctness and unchanged operation benchmarks. Prior rejected normalization, modulus and rollback variants remain rejected.']
(out/'report.md').write_text('\n'.join(lines)+'\n');print(json.dumps(dict(worker_processes=3,worker_samples=144,cpu_clock_samples=sum(x['cpu_clock_samples']for x in scoped),raw_files=len(manifest),performance_qualified=False)))
