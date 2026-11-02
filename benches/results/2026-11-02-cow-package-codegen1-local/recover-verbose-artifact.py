from pathlib import Path
import hashlib,json,re,shutil,subprocess
p=Path(__file__).resolve().parent;root=p.parents[2]
sha=lambda b:hashlib.sha256(b).hexdigest()
old=p/'attempt1-artifact-parser';m=json.loads((old/'provenance.json').read_text());log=(old/'build.log').read_text()
assert m['source_restored']and 'candidate'not in m and 'Finished `bench` profile [optimized]'in log
assert any('--crate-name lin 'in l and '-C codegen-units=1'in l for l in log.splitlines())
expected=json.loads((p.parent/'2026-11-02-rollback-fts-field-impact-bench/input.json').read_text())['candidate']['source_sha256']
assert {str(f.relative_to(root)):sha(f.read_bytes())for f in(root/'src').rglob('*.rs')}==expected
for name,h in m['source_sha256'].items():assert sha(subprocess.check_output(['git','show',m['source_commit']+':'+name],cwd=root))==h
paths=re.findall(r'^\s*Executable `([^`]+)`\s*$',log,re.M);assert len(paths)==1
exe=Path(paths[0]).resolve();assert exe.is_relative_to(Path(m['cargo_target_dir']).resolve())
dest=Path('/tmp/lin-cow-package-codegen1-candidate');assert not dest.exists()
shutil.copy2(exe,dest)
m['candidate']=dict(path=str(dest),sha256=sha(dest.read_bytes()),codegen_units=1)
assert m['candidate']['sha256']!=m['baseline']['sha256']
m['artifact_parser_recovery']=dict(compilation_success=True,original_log_sha256=sha((old/'build.log').read_bytes()),new_compilation=False,reason='verbose Cargo Executable backtick format')
(p/'provenance.json').write_text(json.dumps(m,indent=2)+'\n')
print(json.dumps(dict(recovered=True,source_restored=True,performance_qualification=False)))
