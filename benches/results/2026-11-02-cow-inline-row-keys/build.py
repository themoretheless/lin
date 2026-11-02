from pathlib import Path
import hashlib,json,os,re,shutil,subprocess
p=Path(__file__).resolve().parent;root=p.parents[2]
sha=lambda b:hashlib.sha256(b).hexdigest()
prior=json.loads((root/'benches/results/2026-11-02-cow-package-codegen1-local/provenance.json').read_text())
b=prior['baseline'];assert sha(Path(b['path']).read_bytes())==b['sha256']
for name,h in prior['build_sha256'].items():assert sha((root/name).read_bytes())==h
sources={str(f.relative_to(root)):sha(f.read_bytes())for f in(root/'src').rglob('*.rs')}
bench=root/'benches/compare.rs';original=bench.read_bytes();harness=(root/'benches/results/2026-11-02-rollback-fts-field-impact-bench/compare.rs').read_bytes()
assert sha(harness)==prior['harness_sha256']
original_harness_sha256=sha(harness)
assert harness.count(b'(k.clone(),value)')==1
harness=harness.replace(b'(k.clone(),value)',b'(k.to_string(),value)')
(p/'compare.rs').write_bytes(harness)
command=['cargo','bench','--bench','compare','--no-run','--color','never','--verbose']
env=dict(os.environ,CARGO_TARGET_DIR='/tmp/lin-corrected-update-patch-target-20261101',CARGO_BUILD_JOBS='2')
assert not any(k.startswith('CARGO_PROFILE_')for k in os.environ)
meta=dict(baseline=b,candidate_source_sha256=sources,baseline_source_commit=prior['source_commit'],build_sha256=prior['build_sha256'],harness_sha256=sha(harness),original_harness_sha256=original_harness_sha256,harness_change='durable_json_row owned String boundary only; timed workload unchanged',command=command,rustc=subprocess.check_output(['rustc','-Vv'],text=True),performance_qualified=False)
try:
 bench.write_bytes(harness)
 with(p/'build.log').open('w')as log:run=subprocess.run(command,cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT)
 meta['exit_code']=run.returncode;assert run.returncode==0
 text=(p/'build.log').read_text();matches=re.findall(r'^\s*Executable `([^`]+)`\s*$',text,re.M)
 if not matches:matches=re.findall(r'^\s*Executable (?:\S+ )?\(([^)]+)\)\s*$',text,re.M)
 assert len(matches)==1,matches
 exe=Path(matches[0]).resolve();assert exe.is_relative_to(Path(env['CARGO_TARGET_DIR']).resolve())
 dest=Path('/tmp/lin-cow-inline-row-keys-candidate');assert not dest.exists()
 shutil.copy2(exe,dest);meta['candidate']=dict(path=str(dest),sha256=sha(dest.read_bytes()))
finally:
 bench.write_bytes(original)
 meta['source_restored']=sources=={str(f.relative_to(root)):sha(f.read_bytes())for f in(root/'src').rglob('*.rs')}and bench.read_bytes()==original
 (p/'provenance.json').write_text(json.dumps(meta,indent=2)+'\n')
assert meta['source_restored']
