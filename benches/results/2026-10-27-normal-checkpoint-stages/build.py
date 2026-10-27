from pathlib import Path
import hashlib,json,subprocess,shutil
p=Path(__file__).resolve().parent;store=Path('src/store.rs');before=store.read_bytes();example=Path('examples/tx_checkpoint_stages_probe.rs');assert not example.exists();h=lambda f:hashlib.sha256(Path(f).read_bytes()).hexdigest();old=json.loads(Path('benches/results/2026-10-27-normal-tx-checkpoint-diagnostic/provenance.json').read_text())
m=dict(source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),source_sha256={str(f):h(f)for f in Path('src').rglob('*.rs')},probe_sha256=h(p/'probe.rs'),instrumented_store_sha256=h(p/'instrumented-store.rs'),build_inputs_sha256={n:h(n)for n in ['Cargo.toml','Cargo.lock']},binaries={'original':old['binary']},production_change=False,optimization_candidate=False)
assert m['source_sha256']==old['source_sha256']and m['build_inputs_sha256']==old['build_inputs_sha256']and m['probe_sha256']==old['probe_sha256']and h(old['binary']['path'])==old['binary']['sha256']
try:
 example.write_bytes((p/'probe.rs').read_bytes());store.write_bytes((p/'instrumented-store.rs').read_bytes())
 with(p/'build.log').open('w')as log:r=subprocess.run(['cargo','build','--release','--example','tx_checkpoint_stages_probe','--message-format=json-render-diagnostics'],stdout=log,stderr=subprocess.STDOUT)
 assert r.returncode==0
 artifacts=[]
 for line in(p/'build.log').read_text().splitlines():
  try:row=json.loads(line)
  except ValueError:continue
  if row.get('reason')=='compiler-artifact'and row.get('target',{}).get('name')=='tx_checkpoint_stages_probe'and row.get('executable'):artifacts.append(row['executable'])
 assert len(artifacts)==1;dest=Path('/tmp/lin-normal-checkpoint-stages-probe');shutil.copy2(artifacts[0],dest);m['binaries']['instrumented']=dict(path=str(dest),sha256=h(dest))
finally:
 store.write_bytes(before);example.unlink(missing_ok=True);m['source_restored']=all(h(f)==v for f,v in m['source_sha256'].items());(p/'provenance.json').write_text(json.dumps(m,indent=2)+'\n')
