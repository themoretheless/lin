from pathlib import Path
import json,subprocess,hashlib,re
p=Path(__file__).resolve().parent
inputs=json.loads((p/'source-input.json').read_text())
for source,digest in inputs['source_sha256'].items():assert hashlib.sha256(Path(source).read_bytes()).hexdigest()==digest,source
for source,digest in inputs['generated_sha256'].items():assert hashlib.sha256((p/source).read_bytes()).hexdigest()==digest,source
names=['exec','persist','store','lib'];before={n:Path(f'src/{n}.rs').read_bytes()for n in names};module=Path('src/mutation_profile.rs');assert not module.exists()
try:
 for n in names:Path(f'src/{n}.rs').write_bytes((p/f'instrumented-{n}.rs').read_bytes())
 module.write_bytes((p/'profile.rs').read_bytes())
 with(p/'tests.log').open('w')as log:r=subprocess.run(['cargo','test','--release','--lib','--tests','--color','never'],stdout=log,stderr=subprocess.STDOUT)
 (p/'test-status.json').write_text(json.dumps(dict(exit_code=r.returncode,profile='release',features='default'),indent=2)+'\n');assert r.returncode==0
 counts=[tuple(map(int,m)) for m in re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', (p/'tests.log').read_text())]
 assert counts and sum(c[0] for c in counts)==458 and sum(c[1] for c in counts)==0 and sum(c[2] for c in counts)==2,counts
finally:
 for n,b in before.items():Path(f'src/{n}.rs').write_bytes(b)
 module.unlink(missing_ok=True)
for source,digest in inputs['source_sha256'].items():assert hashlib.sha256(Path(source).read_bytes()).hexdigest()==digest,source
