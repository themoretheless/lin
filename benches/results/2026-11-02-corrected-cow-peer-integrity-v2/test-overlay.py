"""One local source roundtrip, including preservation of an unexpected fixture edit."""
from pathlib import Path
import hashlib,json,subprocess,sys
p=Path(__file__).resolve().parent;root=p.parents[2]
sha=lambda b:hashlib.sha256(b).hexdigest()
before={str(f.relative_to(root)):sha(f.read_bytes())for f in(root/'src').rglob('*.rs')}
fixture=root/'src/__lin_overlay_preservation_probe.rs';assert not fixture.exists()
log=[]
def run(args,ok):
 r=subprocess.run([sys.executable,str(p/'source-overlay.py'),*args],cwd=root,capture_output=True,text=True)
 log.append(dict(args=args,exit_code=r.returncode,stdout=r.stdout,stderr=r.stderr));assert(r.returncode==0)==ok
try:
 run([],True)
 fixture.write_text('// owned negative fixture: must survive rejected restore\n')
 run(['--restore'],False);assert fixture.read_text()=='// owned negative fixture: must survive rejected restore\n'
finally:
 if fixture.exists():fixture.unlink()
 run(['--restore'],True)
 assert {str(f.relative_to(root)):sha(f.read_bytes())for f in(root/'src').rglob('*.rs')}==before
 (p/'overlay-local-proof.json').write_text(json.dumps(dict(source_restored=True,unexpected_edit_preserved=True,benchmark_timings=0,checks=log,source_sha256=before),indent=2)+'\n')
print('PASS exact625 overlay/original restore; unexpected source edit retained on rejection')
