from pathlib import Path
import json,subprocess,hashlib
p=Path(__file__).resolve().parent
src=Path('benches/results/2026-10-24-lz4-wal-cpu/payload-1000.bin')
expected=json.loads(Path('benches/results/2026-10-24-lz4-wal-cpu/fixtures.json').read_text())[0]['payload_sha256'];assert hashlib.sha256(src.read_bytes()).hexdigest()==expected
meta=json.loads((p/'provenance.json').read_text());assert hashlib.sha256(Path('/tmp/lin-sparse-delta-index-io-gate').read_bytes()).hexdigest()==meta['binary_sha256']
status=[]
for series in [1,2]:
 for i in range(1,7):
  root=p/f'io-s{series}-p{i}';assert not root.exists()
  with (p/f's{series}-process-{i}.json').open('w') as out,(p/f's{series}-process-{i}.log').open('w') as log:r=subprocess.run(['/tmp/lin-sparse-delta-index-io-gate',str(src),str(root),str(i)],stdout=out,stderr=log)
  assert r.returncode==0
  d=json.loads((p/f's{series}-process-{i}.json').read_text());assert d['exact_roundtrip'] and all(len(t)==24 for t in d['ns']) and not root.exists()
  status.append({'series':series,'process':i,'exit_code':r.returncode,'owned_directory_removed':True});(p/'status.json').write_text(json.dumps(status,indent=2));print(series,i,'complete',flush=True)
assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in meta['source_sha256'].items())
