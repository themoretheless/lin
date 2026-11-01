from pathlib import Path
import platform,subprocess,hashlib,json,re,runpy,os
assert platform.system()=='Linux'and platform.machine()=='x86_64'
p=Path(__file__).resolve().parent;m=json.loads((p/'provenance.json').read_text());assert m['source_restored']and m['single_binary_for_all_modes'];assert hashlib.sha256(Path(m['binary']).read_bytes()).hexdigest()==m['binary_sha256'];assert json.loads((p/'seed-equivalence/status.json').read_text())['complete']
scope=runpy.run_path(str(p.parent/'2026-11-01-wal-syscall-diagnostic/audit-traces.py'))['scope'];root=p/'setup-traces';root.mkdir(exist_ok=False);rows=[];env=dict(os.environ);env.pop('LIN_BENCH_MIXED_CASE',None);env.pop('LIN_COLUMN_BATCH_FIXTURE',None)
(root/'strace-version.txt').write_text(subprocess.check_output(['strace','--version'],text=True));(root/'filesystem.txt').write_text(subprocess.check_output(['findmnt','-T','/tmp','-o','TARGET,SOURCE,FSTYPE,OPTIONS'],text=True))
for n in [1000,10000]:
 for mode in ['chunked','grouped']:
  for engine in ['lin','sqlite_wal']:
   name=f'{n}-{mode}-{engine}';trace=root/(name+'.strace');cmd=['strace','-f','-yy','-s','160','-o',str(trace),'-e','trace=openat,close,write,writev,pwrite64,pwritev,fsync,fdatasync,lseek,ftruncate,fcntl',m['binary'],'--seed-history-case',str(n),'update',engine,mode]
   r=subprocess.run(cmd,env=env,capture_output=True,text=True);(root/(name+'.stdout')).write_text(r.stdout);(root/(name+'.stderr')).write_text(r.stderr);assert r.returncode==0 and 'LIN_IO_FULL_VALUES_REOPEN_OK'in r.stdout,name
   text=trace.read_text();mutation=scope(text);lines=text.splitlines();starts=[i for i,l in enumerate(lines)if 'write(2<'in l and '"LIN_IO_BEGIN\\n"'in l];assert len(starts)==1;tag='/log>'if engine=='lin'else'db.sqlite-wal>';before=[l for l in lines[:starts[0]]if tag in l];syncs=[l for l in before if re.search(r'\b(fsync|fdatasync)\(',l)];writes=[l for l in before if re.search(r'\b(write|writev|pwrite64|pwritev)\(',l)];sizes=[]
   assert all(re.search(r'= 0$',l)for l in syncs),name
   for line in writes:
    match=re.search(r'= (\d+)$',line);assert match,('split/failed setup write',line);sizes.append(int(match[1]))
   expected=(n//500+2 if mode=='chunked'else 3)if engine=='lin'else 2
   assert len(syncs)==expected,(name,len(syncs),expected)
   actual_syncs=[c['call']for c in mutation if c['call']in ['fsync','fdatasync']];assert actual_syncs==(['fdatasync']if engine=='lin'else['fsync'])
   row=dict(rows=n,mode=mode,engine=engine,command=cmd,exit_code=r.returncode,full_values_reopen=True,setup_wal_sync_calls=len(syncs),setup_wal_write_calls=len(writes),setup_successful_wal_write_bytes=sum(sizes),mutation_syncs=actual_syncs,trace_sha256=hashlib.sha256(trace.read_bytes()).hexdigest());rows.append(row);(root/'status.json').write_text(json.dumps(dict(complete=False,cases=rows,performance_qualification=False),indent=2)+'\n');print(name,'setup WAL history verified',flush=True)
assert len(rows)==8
for n in [1000,10000]:
 sql=[r for r in rows if r['rows']==n and r['engine']=='sqlite_wal'];assert len({(r['setup_wal_sync_calls'],r['setup_wal_write_calls'],r['setup_successful_wal_write_bytes'])for r in sql})==1
(root/'status.json').write_text(json.dumps(dict(complete=True,cases=rows,performance_qualification=False,full_objective_achieved=False),indent=2)+'\n')
