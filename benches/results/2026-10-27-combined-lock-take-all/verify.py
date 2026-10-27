from pathlib import Path
import subprocess,json,hashlib
p=Path(__file__).resolve().parent;paths=['src/exec.rs','tests/exec.rs'];before={n:Path(n).read_bytes()for n in paths};meta={'base_main':'3ac69908090cbec49a51e747db9ec50390d176f8','take_all_head':'c914eea9fe428dd83df3f92b836fc264e4903fa6'}
try:
 for n in paths:Path(n).write_bytes(subprocess.check_output(['git','show',f'c914eea9:{n}']))
 meta['source_sha256']={str(f):hashlib.sha256(f.read_bytes()).hexdigest()for f in Path('src').rglob('*.rs')};meta['test_sha256']={str(f):hashlib.sha256(f.read_bytes()).hexdigest()for f in Path('tests').glob('*.rs')}
 with(p/'tests.log').open('w')as log:r=subprocess.run(['cargo','test','--lib','--tests'],stdout=log,stderr=subprocess.STDOUT)
 (p/'status.json').write_text(json.dumps({'exit_code':r.returncode})+'\n')
 assert r.returncode==0
finally:
 for n,b in before.items():Path(n).write_bytes(b)
 (p/'provenance.json').write_text(json.dumps(meta,indent=2)+'\n')
