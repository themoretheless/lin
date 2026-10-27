from pathlib import Path
import hashlib,json,subprocess,shutil
p=Path(__file__).resolve().parent;f=Path('examples/tx_normal_checkpoint_probe.rs');assert not f.exists();h=lambda f:hashlib.sha256(Path(f).read_bytes()).hexdigest()
m=dict(source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),source_sha256={str(f):h(f)for f in Path('src').rglob('*.rs')},probe_sha256=h(p/'probe.rs'),build_inputs_sha256={n:h(n)for n in ['Cargo.toml','Cargo.lock']},production_source_changed=False,optimization_candidate=False)
try:
 f.write_bytes((p/'probe.rs').read_bytes())
 with(p/'build.log').open('w')as log:r=subprocess.run(['cargo','build','--release','--example','tx_normal_checkpoint_probe','--message-format=json-render-diagnostics'],stdout=log,stderr=subprocess.STDOUT)
 assert r.returncode==0
 artifacts=[]
 for line in(p/'build.log').read_text().splitlines():
  try:row=json.loads(line)
  except ValueError:continue
  if row.get('reason')=='compiler-artifact'and row.get('target',{}).get('name')=='tx_normal_checkpoint_probe'and row.get('executable'):artifacts.append(row['executable'])
 assert len(artifacts)==1;dest=Path('/tmp/lin-normal-tx-checkpoint-probe');shutil.copy2(artifacts[0],dest);m['binary']=dict(path=str(dest),sha256=h(dest))
finally:
 f.unlink(missing_ok=True);m['source_restored']=all(h(f)==d for f,d in m['source_sha256'].items());(p/'provenance.json').write_text(json.dumps(m,indent=2)+'\n')
