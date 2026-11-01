from pathlib import Path
import subprocess,json,hashlib,platform,sys,os,re,collections
p=Path(__file__).resolve().parent;functional='--functional'in sys.argv
assert functional or(platform.system()=='Linux'and platform.machine()=='x86_64')
provenance=json.loads((p/'provenance.json').read_text());binary=provenance['binary'];assert hashlib.sha256(Path(binary).read_bytes()).hexdigest()==provenance['binary_sha256']
root=p/('functional'if functional else'traces');root.mkdir(exist_ok=False)
env=dict(os.environ,LIN_BENCH_SKIP_MYSQL='1',LIN_BENCH_SKIP_PG='1');rows=[]
if not functional:
 (root/'strace-version.txt').write_text(subprocess.check_output(['strace','--version'],text=True))
 (root/'filesystem.txt').write_text(subprocess.check_output(['findmnt','-T','/tmp','-o','TARGET,SOURCE,FSTYPE,OPTIONS'],text=True))
for n in [1000,10000]:
 for op in ['update','delete','commit','rollback']:
  for engine in ['lin','sqlite_wal']:
   name=f'{n}-{op}-{engine}';cmd=[binary,'--syscall-case',str(n),op,engine];trace=root/(name+'.strace')
   if not functional:cmd=['strace','-f','-yy','-s','160','-o',str(trace),'-e','trace=openat,close,write,writev,pwrite64,pwritev,fsync,fdatasync,ftruncate,fallocate,lseek,fcntl,unlink,unlinkat,rename,renameat,mkdir',*cmd]
   r=subprocess.run(cmd,env=env,capture_output=True,text=True);(root/(name+'.stdout')).write_text(r.stdout);(root/(name+'.stderr')).write_text(r.stderr)
   assert r.returncode==0 and 'LIN_IO_FULL_VALUES_REOPEN_OK'in r.stdout,name
   row=dict(rows=n,operation=op,engine=engine,command=cmd,exit_code=r.returncode,full_values_reopen=True)
   if not functional:
    lines=trace.read_text().splitlines();starts=[i for i,l in enumerate(lines)if 'write(2<'in l and '"LIN_IO_BEGIN\\n"'in l];ends=[i for i,l in enumerate(lines)if 'write(2<'in l and '"LIN_IO_END\\n"'in l]
    assert len(starts)==len(ends)==1 and starts[0]<ends[0],name
    scoped=lines[starts[0]+1:ends[0]];(root/(name+'.scope')).write_text('\n'.join(scoped)+'\n')
    calls=collections.Counter()
    for line in scoped:
     m=re.search(r'\b([a-z][a-z0-9_]*)\(',line)
     if m:calls[m[1]]+=1
    assert not any('= -1 'in l for l in scoped),name
    row.update(scope_syscalls=dict(calls),scope_lines=len(scoped),trace_sha256=hashlib.sha256(trace.read_bytes()).hexdigest())
   rows.append(row);(root/'status.json').write_text(json.dumps(dict(complete=False,cases=rows,performance_qualification=False),indent=2)+'\n')
   print(name,'full values verified',flush=True)
assert len(rows)==16
(root/'status.json').write_text(json.dumps(dict(complete=True,cases=rows,performance_qualification=False,full_objective_achieved=False),indent=2)+'\n')
