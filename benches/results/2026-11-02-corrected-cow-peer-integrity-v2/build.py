from pathlib import Path
import hashlib,json,subprocess,shutil,os
p=Path(__file__).resolve().parent;h=lambda f:hashlib.sha256(Path(f).read_bytes()).hexdigest();files=list(Path('src').rglob('*.rs'))+[Path(n)for n in ['Cargo.toml','Cargo.lock','examples/peer_bench.rs','scripts/bench-peers.py','scripts/benchmark_source_integrity.py','scripts/test_benchmark_source_integrity.py']]
assert not any(k.startswith('CARGO_PROFILE_')for k in os.environ)and not os.environ.get('RUSTFLAGS')and not os.environ.get('CARGO_ENCODED_RUSTFLAGS')
m=dict(runtime_reference=json.loads((p/'input.json').read_text())['runtime_reference'],source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),source_sha256={str(f):h(f)for f in files},rustc=subprocess.check_output(['rustc','-Vv'],text=True))
frozen=json.loads(Path('benches/results/2026-11-01-corrected-cow-full-engine/source-inputs.json').read_text())
assert {str(f):h(f)for f in Path('src').rglob('*.rs')}==frozen['candidate']['source_sha256']
with(p/'build.log').open('w')as log:r=subprocess.run(['cargo','build','--release','--example','peer_bench','--message-format=json-render-diagnostics'],stdout=log,stderr=subprocess.STDOUT,env=dict(os.environ,CARGO_TARGET_DIR='target/cow-peer-integrity-v2',CARGO_BUILD_JOBS='2'))
assert r.returncode==0
artifacts=[]
for line in(p/'build.log').read_text().splitlines():
 try:record=json.loads(line)
 except ValueError:continue
 if record.get('reason')=='compiler-artifact'and record.get('target',{}).get('name')=='peer_bench'and record.get('executable'):artifacts.append(record['executable'])
assert len(artifacts)==1;destination=Path('/tmp/lin-cow-corrected-peer-built');shutil.copy2(artifacts[0],destination);m['binary']=dict(path=str(destination),sha256=h(destination));m['source_unchanged']=all(h(f)==v for f,v in m['source_sha256'].items());assert m['source_unchanged'];(p/'build-provenance.json').write_text(json.dumps(m,indent=2)+'\n')
