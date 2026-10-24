from pathlib import Path
import subprocess,json,hashlib
p=Path(__file__).resolve().parent
s=Path('src/persist.rs').read_text();a=s.index('fn append_sparse_rows_serial(');b=s.index('fn append_sparse_worker(',a)
assert s[a:b].replace('fn append_sparse_rows_serial(','fn encode_sparse_reference(').rstrip()==(p/'sparse-reference.rs').read_text().rstrip()
meta={'source_commit':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'source_sha256':{str(f):hashlib.sha256(f.read_bytes()).hexdigest() for f in sorted(Path('src').rglob('*.rs'))},'gate_sha256':hashlib.sha256((p/'gate.rs').read_bytes()).hexdigest(),'reference_sha256':hashlib.sha256((p/'sparse-reference.rs').read_bytes()).hexdigest()}
(p/'provenance.json').write_text(json.dumps(meta,indent=2)+'\n')
status={}
for i in range(1,7):
 with (p/f'process-{i}.jsonl').open('w') as out,(p/f'process-{i}.log').open('w') as log:r=subprocess.run(['/tmp/lin-wal-dictionary-cpu-gate-v2',str(i)],stdout=out,stderr=log)
 status[str(i)]=r.returncode;(p/'status.json').write_text(json.dumps(status,indent=2))
 if r.returncode:raise SystemExit(r.returncode)
 docs=[json.loads(line) for line in (p/f'process-{i}.jsonl').read_text().splitlines()];assert len(docs)==4
 assert all(d['exact_bits_roundtrip'] and len(d['sparse_ns'])==len(d['candidate_ns'])==12 for d in docs)
 print(i,'validated',flush=True)
assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in meta['source_sha256'].items())
