from pathlib import Path
import os,subprocess,json,hashlib
p=Path(__file__).resolve().parent;m=json.loads((p/'provenance.json').read_text());assert m['source_restored']and json.loads((p/'functional/status.json').read_text())['complete'];assert hashlib.sha256(Path(m['binary']).read_bytes()).hexdigest()==m['binary_sha256'];rows=[]
for n in [1000,10000]:
 root=p/f'fixture-{n}-grouped';assert not root.exists();env=dict(os.environ,LIN_COLUMN_BATCH_FIXTURE=str(root));cmd=[m['binary'],'--seed-history-case',str(n),'update','lin','grouped'];r=subprocess.run(cmd,env=env,capture_output=True,text=True);(p/f'fixture-{n}.stdout').write_text(r.stdout);(p/f'fixture-{n}.stderr').write_text(r.stderr);assert r.returncode==0 and 'LIN_IO_FULL_VALUES_REOPEN_OK'in r.stdout;assert len(json.loads((root/'expected.json').read_text()))==n
 files={f.name:dict(bytes=f.stat().st_size,sha256=hashlib.sha256(f.read_bytes()).hexdigest())for f in root.iterdir()if f.is_file()};rows.append(dict(rows=n,command=cmd,exit_code=r.returncode,full_values_reopen=True,files=files))
(p/'fixture-status.json').write_text(json.dumps(dict(complete=True,cases=rows,legacy_reader_pending=True,performance_qualified=False),indent=2)+'\n')
