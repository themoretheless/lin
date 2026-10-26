from pathlib import Path
import subprocess,json,hashlib,shutil
p=Path(__file__).resolve().parent;files={'bounded_dictionary_choice_cpu.rs':'gate.rs','dictionary_choice_reference.rs':'reference.rs','dictionary_choice_candidate.rs':'dictionary.rs'};assert all(not Path('examples',f).exists() for f in files)
meta=dict(source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),source_sha256={str(f):hashlib.sha256(f.read_bytes()).hexdigest() for f in Path('src').rglob('*.rs')},gate_sha256={f:hashlib.sha256((p/f).read_bytes()).hexdigest() for f in files.values()},rustc=subprocess.check_output(['rustc','-Vv'],text=True))
try:
 for target,source in files.items():Path('examples',target).write_bytes((p/source).read_bytes())
 with (p/'build.log').open('w') as log:r=subprocess.run(['cargo','build','--release','--example','bounded_dictionary_choice_cpu'],stdout=log,stderr=subprocess.STDOUT)
 meta['build_exit']=r.returncode;assert r.returncode==0;shutil.copy2('target/release/examples/bounded_dictionary_choice_cpu','/tmp/lin-bounded-dictionary-choice-cpu');meta['binary_sha256']=hashlib.sha256(Path('/tmp/lin-bounded-dictionary-choice-cpu').read_bytes()).hexdigest()
finally:
 for target in files:Path('examples',target).unlink(missing_ok=True)
 (p/'provenance.json').write_text(json.dumps(meta,indent=2))
