from pathlib import Path
import hashlib,json,math,statistics,subprocess,tarfile
p=Path(__file__).resolve().parent;root=p.parents[2]
run_id=41386055044;head='8acc59abeab4c72f4ac250c2877bab3ab3c95528'
sha=lambda b:hashlib.sha256(b).hexdigest()
metadata=json.loads(subprocess.check_output(['gh','run','view',str(run_id),'--json','databaseId,headSha,status,conclusion,url,jobs'],cwd=root,text=True))
assert metadata['status']=='completed','poll the same live run; do not download partial artifacts'
assert metadata['headSha']==head and metadata['conclusion']=='success',metadata
work=Path(f'/tmp/lin-fixed-eof-sync-{run_id}');binary_dir=Path(f'/tmp/lin-fixed-eof-sync-executable-{run_id}')
assert not work.exists()and not binary_dir.exists(),'preserve previous download'
subprocess.run(['gh','run','download',str(run_id),'--name','fixed-eof-sync-diagnostic','--dir',str(work)],cwd=root,check=True)
subprocess.run(['gh','run','download',str(run_id),'--name','fixed-eof-exact-linux-executable','--dir',str(binary_dir)],cwd=root,check=True)
load=lambda name:json.loads((work/name).read_text())
inputs=load('input.json');env=load('environment.json');records=load('process-status.json');audit=load('audit.json')
frozen=json.loads(subprocess.check_output(['git','show',head+':'+str(p.relative_to(root))+'/input.json'],cwd=root,text=True));assert inputs==frozen
for name,h in inputs['files_sha256'].items():
 blob=subprocess.check_output(['git','show',head+':'+name],cwd=root);assert sha(blob)==h
 local=work/Path(name).name
 if local.exists():assert sha(local.read_bytes())==h,name
bins=[f for f in binary_dir.rglob('*')if f.is_file()];assert len(bins)==1
binary=bins[0].read_bytes();assert binary[:4]==b'\x7fELF'and sha(binary)==env['binary']['sha256']
assert load('status.json')==dict(complete=True,processes=36,observations=2304,source_unchanged=True,owned_files_removed=True,performance_qualification=False)
assert len(records)==36 and audit['audit_complete']and audit['observations']==2304
import sys
sys.path.insert(0,str(root/'scripts'))
from benchmark_role_balance import schedule,orders_from_records
roles=['append_a','append_b','fixed_sparse_a','fixed_sparse_b','fixed_zeroed_a','fixed_zeroed_b']
assert [{k:r[k]for k in ['round','workload','role']}for r in records]==schedule([dict(id='point_sync')],roles,rounds=6)
orders_from_records(records,['point_sync'],roles,'workload',rounds=6)
med={};observations=0;original_root=None
for r in records:
 mode=r['role'].rsplit('_',1)[0];name=f"raw/{r['round']:02}-{r['role']}.json";assert r['raw']==name
 d=load(name);values=d['latencies_ns'];assert len(values)==64 and all(type(v)is int and v>0 for v in values)
 assert d['mode']==mode and d['samples']==64 and d['payload_bytes']==4125 and d['logical_end']==280384
 assert d['physical_bytes']==(280384 if mode=='append'else 8388608)and d['full_readback_verified']and d['owned_file_removed']
 command=r['command'];assert command[:2]==[env['binary']['path'],mode]and len(command)==3
 target=Path(command[2]);assert target.parts[-5:]==('benches','results',p.name,'owned-files',f"{r['round']:02}-{r['role']}.bin")
 current_root=str(target.parents[3]);original_root=original_root or current_root;assert current_root==original_root
 assert r['exit_code']==0 and r['binary_sha256']==env['binary']['sha256']
 median=statistics.median(values);assert median==r['median_ns'];med[r['round'],r['role']]=median;observations+=len(values)
summary={}
for mode in ['append','fixed_sparse','fixed_zeroed']:
 pairs=[]
 for rnd in range(1,7):
  base=med[rnd,'append_a'];a=med[rnd,mode+'_a'];b=med[rnd,mode+'_b']
  pairs.append(dict(round=rnd,mode_a_ns=a,mode_b_ns=b,append_ns=base,gain_percent=100*(1-a/base),same_mode_control_percent=100*(1-b/a)))
 summary[mode]=dict(gain_percent=statistics.median(r['gain_percent']for r in pairs),same_mode_aa_percent=statistics.median(abs(r['same_mode_control_percent'])for r in pairs),positive_pairs=sum(r['gain_percent']>0 for r in pairs),pairs=pairs)
assert observations==2304 and summary==audit['summary']
out=root/'benches/results/2026-11-02-fixed-eof-sync-linux';out.mkdir(exist_ok=False)
files=sorted(f for f in work.rglob('*')if f.is_file());assert not any(f.is_symlink()for f in work.rglob('*'))
manifest={str(f.relative_to(work)):dict(bytes=f.stat().st_size,sha256=sha(f.read_bytes()))for f in files}
with tarfile.open(out/'raw.tar.gz','w:gz')as t:
 for f in files:t.add(f,arcname=str(f.relative_to(work)),recursive=False)
with tarfile.open(out/'raw.tar.gz')as t:
 assert sorted(t.getnames())==sorted(manifest)
 for member in t.getmembers():assert sha(t.extractfile(member).read())==manifest[member.name]['sha256']
for name,value in [('run-metadata.json',metadata),('raw-manifest.json',manifest),('summary.json',summary),('independent-audit.json',dict(files=len(files),observations=2304,frozen_git_blobs_verified=True,exact_elf_sha256=sha(binary),elf_executed_locally=False,raw_numerical_recompute_identical=True,performance_qualification=False,wal_format_proven=False))]:
 (out/name).write_text(json.dumps(value,indent=2)+'\n')
lines=['# Fixed-EOF fdatasync diagnostic','','One Linux series; diagnostic only, no production WAL change or adoption. Positive gain means lower median latency relative to growing EOF.','','| Mode | Gain % | Same-mode AA % | Positive pairs |','|---|---:|---:|---:|']
for mode,r in summary.items():lines.append(f"| {mode} | {r['gain_percent']:+.3f} | {r['same_mode_aa_percent']:.3f} | {r['positive_pairs']}/6 |")
lines+=['','All 2304 observations, 36 commands, frozen input blobs and exact Linux executable hash were independently checked. Readback is not a crash/recovery or equivalent-WAL proof. No conclusion about full database superiority follows.']
(out/'report.md').write_text('\n'.join(lines)+'\n');print(json.dumps(dict(archived_files=len(files),observations=observations,summary=summary)))
