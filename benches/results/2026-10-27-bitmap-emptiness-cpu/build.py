from pathlib import Path
import subprocess,json,hashlib,shutil
p=Path(__file__).resolve().parent;f=Path('src/index.rs');example=Path('examples/bitmap_emptiness_cpu.rs');assert f.read_bytes()==(p/'before-index.rs').read_bytes() and not example.exists();meta={'source_commit':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'source_sha256':{str(f):hashlib.sha256(f.read_bytes()).hexdigest()for f in Path('src').rglob('*.rs')},'gate_sha256':hashlib.sha256((p/'gate.rs').read_bytes()).hexdigest()}
try:
 f.write_bytes((p/'candidate-index.rs').read_bytes());example.write_bytes((p/'gate.rs').read_bytes())
 with (p/'build.log').open('w')as log:r=subprocess.run(['cargo','build','--release','--example','bitmap_emptiness_cpu'],stdout=log,stderr=subprocess.STDOUT)
 meta['build_exit']=r.returncode;assert r.returncode==0;binary='/tmp/lin-bitmap-emptiness-cpu';shutil.copy2('target/release/examples/bitmap_emptiness_cpu',binary);meta['binary_sha256']=hashlib.sha256(Path(binary).read_bytes()).hexdigest()
finally:
 f.write_bytes((p/'before-index.rs').read_bytes());example.unlink(missing_ok=True);(p/'provenance.json').write_text(json.dumps(meta,indent=2)+'\n')
print('built; original source restored',flush=True)
