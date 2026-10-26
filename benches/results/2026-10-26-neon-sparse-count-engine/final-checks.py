from pathlib import Path
import subprocess,json,re,hashlib
r=Path(__file__).resolve().parent;out={}
for name,args in [('no-default',['cargo','check','--lib','--no-default-features']),('optional',['cargo','check','--lib','--features','gpu,parallel,compress-wal,compress-lz4,compress-flate']),('final-bench',['cargo','bench','--bench','compare','--no-run'])]:
 with (r/(name+'.log')).open('w') as log:p=subprocess.run(args,stdout=log,stderr=subprocess.STDOUT)
 out[name]={'exit_code':p.returncode};(r/'final-checks.json').write_text(json.dumps(out,indent=2));assert p.returncode==0;print(name,'passed',flush=True)
exe=re.search(r'Executable benches/compare.rs \(([^)]+)\)',(r/'final-bench.log').read_text())[1]
hash=lambda p:hashlib.sha256(Path(p).read_bytes()).hexdigest()
out['bench_binary_equal_measured']=hash(exe)==hash('/tmp/lin-neon-sparse-count-engine-candidate');assert out['bench_binary_equal_measured']
example=Path('examples/neon_sparse_count_interleaved.rs');assert not example.exists()
try:
 example.write_bytes((r/'interleaved-worker.rs').read_bytes())
 with (r/'final-interleaved.log').open('w') as log:p=subprocess.run(['cargo','build','--release','--example','neon_sparse_count_interleaved'],stdout=log,stderr=subprocess.STDOUT)
 assert p.returncode==0
 out['interleaved_binary_equal_measured']=hash('target/release/examples/neon_sparse_count_interleaved')==hash('/tmp/lin-neon-sparse-count-interleaved-ab-candidate');assert out['interleaved_binary_equal_measured']
finally:example.unlink(missing_ok=True)
for n in ['persist']:(r/f'final-candidate-{n}.rs').write_bytes(Path(f'src/{n}.rs').read_bytes())
out['source_sha256']={str(f):hash(f) for f in Path('src').rglob('*.rs')}
(r/'final-checks.json').write_text(json.dumps(out,indent=2));print('release binaries unchanged after test-only additions',flush=True)
