from pathlib import Path
import subprocess,json,hashlib,re
r=Path(__file__).resolve().parent;verification={};example=Path('examples/bitmap_sample_once.rs');assert not example.exists()
try:
 for name,cmd in [('workspace',['cargo','test','--workspace']),('peer',['cargo','build','--release','--example','peer_bench']),('compare',['cargo','bench','--bench','compare','--no-run']),('worker',['cargo','build','--release','--example','bitmap_sample_once'])]:
  if name=='worker':example.write_bytes((r/'worker.rs').read_bytes())
  with (r/f'final-{name}.log').open('w') as log:p=subprocess.run(cmd,stdout=log,stderr=subprocess.STDOUT)
  verification[name+'_exit_code']=p.returncode;assert p.returncode==0
  print(name,'passed',flush=True)
 expected_peer=json.loads((r/'peer-read/binary-sha256.json').read_text())['sha256'];expected_compare=json.loads((r/'engine-provenance.json').read_text())['candidate']['binary_sha256'];expected_worker=json.loads((r/'provenance.json').read_text())['candidate']['binary_sha256']
 exe=re.search(r'Executable benches/compare.rs \(([^)]+)\)',(r/'final-compare.log').read_text())[1]
 for name,path,expected in [('peer','target/release/examples/peer_bench',expected_peer),('compare',exe,expected_compare),('worker','target/release/examples/bitmap_sample_once',expected_worker)]:
  actual=hashlib.sha256(Path(path).read_bytes()).hexdigest();verification[name+'_binary_sha256']=actual;verification[name+'_matches_measured_binary']=actual==expected
 verification['source_sha256']={str(f):hashlib.sha256(f.read_bytes()).hexdigest() for f in Path('src').rglob('*.rs')};verification['source_difference']='Only cfg(test) oracle fixtures changed after measurement: isolate late boolean and cross 10000-key boundary; release code unchanged.'
 (r/'final-index.rs').write_bytes(Path('src/index.rs').read_bytes())
 (r/'final-verification.json').write_text(json.dumps(verification,indent=2));print(verification,flush=True)
 assert all(verification[k+'_matches_measured_binary'] for k in ['peer','compare','worker'])
finally:example.unlink(missing_ok=True)
