from pathlib import Path
import tempfile,json,subprocess,hashlib,shutil
p=Path(__file__).resolve().parent;q=p.parent/'2026-11-01-columnar-batch-wal';sha=lambda b:hashlib.sha256(b).hexdigest();cases=json.loads((p/'cases.json').read_text());roles=json.loads((p/'roles.json').read_text())
with tempfile.TemporaryDirectory(prefix='lin-history-verifier-')as td:
 parent=Path(td);d=parent/'diagnostic';d.mkdir();legacy=parent/q.name;legacy.mkdir()
 helper=parent/'2026-11-01-wal-syscall-diagnostic';helper.mkdir();shutil.copy2(p.parent/helper.name/'audit-traces.py',helper/'audit-traces.py')
 for f in ['full-release.log','mutant-status.json']:shutil.copy2(q/f,legacy/f)
 for f in ['provenance.json','frozen.json','timestamp-anchor.json','cases.json','roles.json','compare.rs']:shutil.copy2(p/f,d/f)
 for folder in ['seed-equivalence','smoke']:shutil.copytree(p/folder,d/folder)
 m=json.loads((d/'provenance.json').read_text());smokes=json.loads((d/'smoke/status.json').read_text())
 for r in smokes['cases']:r['command'][-1]=str(d/'smoke'/r['output'])
 (d/'smoke/status.json').write_text(json.dumps(smokes))
 traces=d/'setup-traces';traces.mkdir();rows=[]
 for n in [1000,10000]:
  for mode in ['chunked','grouped']:
   for engine in ['lin','sqlite_wal']:
    name=f'{n}-{mode}-{engine}';path='/tmp/synthetic/log'if engine=='lin'else'/tmp/synthetic/db.sqlite-wal';sync='fdatasync'if engine=='lin'else'fsync';count=(n//500+2 if mode=='chunked'else 3)if engine=='lin'else 2
    text=f'11 write(4<{path}>, "x", 1) = 1\n'+(f'11 {sync}(4<{path}>) = 0\n'*count)+'11 write(2<pipe:[2]>, "LIN_IO_BEGIN\\n", 13) = 13\n'+f'11 {sync}(4<{path}>) = 0\n'+'11 write(2<pipe:[2]>, "LIN_IO_END\\n", 11) = 11\n'
    f=traces/(name+'.strace');f.write_text(text);(traces/(name+'.stdout')).write_text('SYNTHETIC LIN_IO_FULL_VALUES_REOPEN_OK\n');rows.append(dict(rows=n,mode=mode,engine=engine,exit_code=0,setup_wal_sync_calls=count,setup_wal_write_calls=1,setup_successful_wal_write_bytes=1,trace_sha256=sha(f.read_bytes())))
 (traces/'status.json').write_text(json.dumps(dict(complete=True,synthetic=True,cases=rows)))
 root=d/'measurement';root.mkdir();records=[]
 for rnd in range(1,13):
  for i,c in enumerate(cases[rnd%8:]+cases[:rnd%8]):
   shift=(rnd+i)%5
   for role in roles[shift:]+roles[:shift]:
    name=f'{len(records):04}';out=root/name;out.mkdir();case=f"compare/seed_history_{role['mode']}_{c['operation']}_{c['size']}/{role['engine']}";cmd=[m['binary'],'--filter',case,'--samples','8','--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output',str(out)]
    data=dict(id='synthetic-'+name,status='complete',synthetic=True,cases=[dict(id=case)],observations=[dict(case=case,metric='wall',operations=1,availability=dict(state='available'),value=80 if role['id']=='lin_grouped'else 100)for _ in range(8)])
    (out/'run.json').write_text(json.dumps(data));records.append(dict(round=rnd,workload=c['id'],role=role['id'],case=case,command=cmd,exit_code=0,binary_sha256=m['binary_sha256'],output=name))
 status=root/'process-status.json';status.write_text(json.dumps(records));subprocess.run(['python3',str(p/'analyze.py'),str(d)],check=True,capture_output=True)
 code=(p/'verify.py').read_text().replace('p=Path(__file__).resolve().parent;',f'p=Path({str(d)!r});')
 def run(ok):
  r=subprocess.run(['python3','-c',code],capture_output=True,text=True);assert(r.returncode==0)==ok,r.stderr[-2000:]
 run(True);original=status.read_text();x=json.loads(original);x[0]['command'][2]='wrong';status.write_text(json.dumps(x));run(False);status.write_text(original)
 s=root/'summary.json';x=json.loads(s.read_text());x[cases[0]['id']]['series1']['pairs'][0]['lin_grouped_ns']=1;s.write_text(json.dumps(x));run(False)
(p/'test-verifier-status.json').write_text(json.dumps(dict(passed=True,synthetic=True,real_timings=False,checks=['full-synthetic-proof-accepted','wrong-command-rejected','tampered-pair-rejected']),indent=2)+'\n');print('PASS synthetic end-to-end independent history verifier and corruptions')
