from pathlib import Path
import subprocess,json,hashlib
p=Path(__file__).resolve().parent;candidate=p.parent/'2026-10-27-incremental-rollback-engine';files=[Path('src/exec.rs'),Path('src/store.rs')];before={f:f.read_bytes()for f in files};h=lambda b:hashlib.sha256(b).hexdigest();m=dict(source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),source_sha256={str(f):h(f.read_bytes())for f in Path('src').rglob('*.rs')},candidate_sources_sha256={str(f):h((candidate/('candidate-'+f.name)).read_bytes())for f in files},oracle_sha256=h((p/'oracle.rs').read_bytes()),performance_qualified=False)
try:
 for f in files:f.write_bytes((candidate/('candidate-'+f.name)).read_bytes())
 files[1].write_text(files[1].read_text()+'\n'+(p/'oracle.rs').read_text())
 with(p/'tests.log').open('w')as log:r=subprocess.run(['cargo','test','--release','--lib','store::incremental_rollback_fold_oracle','--','--nocapture'],stdout=log,stderr=subprocess.STDOUT)
 m['exit_code']=r.returncode;assert r.returncode==0;text=(p/'tests.log').read_text();parts=text.split('ROLLBACK_FTS_FOLD_PROBE ');assert len(parts)==2;result=json.JSONDecoder().raw_decode(parts[1])[0];(p/'result.json').write_text(json.dumps(result,indent=2)+'\n')
finally:
 for f,data in before.items():f.write_bytes(data)
 m['source_restored']=all(h(Path(f).read_bytes())==v for f,v in m['source_sha256'].items());(p/'provenance.json').write_text(json.dumps(m,indent=2)+'\n')
