from pathlib import Path
import subprocess,hashlib,json
p=Path(__file__).resolve().parent;m=json.loads((p/'provenance.json').read_text());assert m['source_restored'] and not m['fsync_candidate_included'];binary=m['binary'];assert hashlib.sha256(Path(binary).read_bytes()).hexdigest()==m['binary_sha256'];root=p/'functional';root.mkdir(exist_ok=False);rows=[]
for n in [1000,10000]:
 for op in ['update','delete','commit','rollback']:
  for engine in ['lin','sqlite_wal']:
   for mode in ['chunked','grouped']:
    name=f'{n}-{op}-{engine}-{mode}';cmd=[binary,'--seed-history-case',str(n),op,engine,mode];r=subprocess.run(cmd,capture_output=True,text=True);(root/(name+'.stdout')).write_text(r.stdout);(root/(name+'.stderr')).write_text(r.stderr)
    rows.append(dict(name=name,command=cmd,exit_code=r.returncode,full_values_reopen='LIN_IO_FULL_VALUES_REOPEN_OK'in r.stdout));(root/'status.json').write_text(json.dumps(dict(complete=False,cases=rows,performance_qualification=False),indent=2)+'\n');assert r.returncode==0 and rows[-1]['full_values_reopen'],name
    print(name,'full values verified',flush=True)
assert len(rows)==32
(root/'status.json').write_text(json.dumps(dict(complete=True,cases=rows,performance_qualification=False,full_objective_achieved=False),indent=2)+'\n')
