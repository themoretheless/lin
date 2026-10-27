from pathlib import Path
import subprocess,json,hashlib,platform
p=Path(__file__).resolve().parent;src=p.parent/'2026-10-27-bitset-exact-cardinality';meta=dict(platform=platform.platform(),rustc=subprocess.check_output(['rustc','-Vv'],text=True),component_only=True,variants={})
for role in ['baseline','candidate']:
 source=(src/f'{role}-index.rs').read_text();start=source.index('#[derive(Debug, Clone, PartialEq, Eq)]\npub struct BitSet');end=source.index('/// Pick one seek',start);part=source[start:end];full=part+'\n'+(p/'harness.rs').read_text();f=p/f'{role}.rs';f.write_text(full);binary=Path(f'/tmp/lin-bitset-cardinality-{role}')
 with(p/f'build-{role}.log').open('w')as log:r=subprocess.run(['rustc','--edition=2024','-C','opt-level=3',str(f),'-o',str(binary)],stdout=log,stderr=subprocess.STDOUT)
 assert r.returncode==0;meta['variants'][role]=dict(binary=str(binary),binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),source_sha256=hashlib.sha256(full.encode()).hexdigest(),original_index_sha256=hashlib.sha256(source.encode()).hexdigest())
(p/'provenance.json').write_text(json.dumps(meta,indent=2)+'\n')
