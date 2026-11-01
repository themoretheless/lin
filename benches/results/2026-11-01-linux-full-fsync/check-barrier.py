from pathlib import Path
import json,hashlib,subprocess,platform,runpy
assert platform.system()=='Linux'
p=Path(__file__).resolve().parent;scope=runpy.run_path(str(p.parent/'2026-11-01-wal-syscall-diagnostic/audit-traces.py'))['scope'];m=json.loads((p/'provenance.json').read_text());root=p/'barrier-check';root.mkdir(exist_ok=False);results={}
for role in ['baseline','candidate','missing_barrier']:
 b=m['binaries'][role];assert hashlib.sha256(Path(b['path']).read_bytes()).hexdigest()==b['sha256']
 trace=root/(role+'.strace');cmd=['strace','-f','-yy','-s','160','-o',str(trace),'-e','trace=write,writev,pwrite64,pwritev,fsync,fdatasync',b['path'],'--syscall-case','1000','update','lin']
 r=subprocess.run(cmd,capture_output=True,text=True);(root/(role+'.stdout')).write_text(r.stdout);(root/(role+'.stderr')).write_text(r.stderr);assert r.returncode==0 and 'LIN_IO_FULL_VALUES_REOPEN_OK'in r.stdout
 calls=scope(trace.read_text());syncs=[x['call']for x in calls if x['call']in ['fsync','fdatasync']];assert syncs==({'baseline':['fdatasync'],'candidate':['fsync'],'missing_barrier':[]}[role]),syncs
 rejected=False
 try:assert len(syncs)==1,'missing durable barrier'
 except AssertionError:rejected=True
 assert rejected==(role=='missing_barrier')
 results[role]=dict(command=cmd,exit_code=r.returncode,syncs=syncs,missing_barrier_rejected=rejected,full_values_reopen=True,trace_sha256=hashlib.sha256(trace.read_bytes()).hexdigest())
(root/'status.json').write_text(json.dumps(dict(complete=True,compiled_negative_caught=True,results=results),indent=2)+'\n')
