from pathlib import Path
import hashlib,json,subprocess,shutil
p=Path(__file__).resolve().parent;h=lambda f:hashlib.sha256(Path(f).read_bytes()).hexdigest();files=list(Path('src').rglob('*.rs'))+[Path(n)for n in ['Cargo.toml','Cargo.lock','examples/peer_bench.rs','scripts/bench-peers.py']]
m=dict(source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),source_sha256={str(f):h(f)for f in files},rustc=subprocess.check_output(['rustc','-Vv'],text=True))
with(p/'build.log').open('w')as log:r=subprocess.run(['cargo','build','--release','--example','peer_bench','--message-format=json-render-diagnostics'],stdout=log,stderr=subprocess.STDOUT)
assert r.returncode==0
artifacts=[]
for line in(p/'build.log').read_text().splitlines():
 try:record=json.loads(line)
 except ValueError:continue
 if record.get('reason')=='compiler-artifact'and record.get('target',{}).get('name')=='peer_bench'and record.get('executable'):artifacts.append(record['executable'])
assert len(artifacts)==1;destination=Path('/tmp/lin-corrected-peer-built');shutil.copy2(artifacts[0],destination);m['binary']=dict(path=str(destination),sha256=h(destination));m['source_unchanged']=all(h(f)==v for f,v in m['source_sha256'].items());assert m['source_unchanged'];(p/'build-provenance.json').write_text(json.dumps(m,indent=2)+'\n')
