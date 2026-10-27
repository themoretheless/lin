from pathlib import Path
import json,hashlib,subprocess
p=Path(__file__).resolve().parent;candidate=p.parent/'2026-10-27-incremental-rollback-engine';files=[Path('src/exec.rs'),Path('src/store.rs')];before={f:f.read_bytes()for f in files};h=lambda b:hashlib.sha256(b).hexdigest();m=dict(source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),source_sha256={str(f):h(f.read_bytes())for f in Path('src').rglob('*.rs')},candidate_sources_sha256={str(f):h((candidate/('candidate-'+f.name)).read_bytes())for f in files},oracle_sha256=h((p/'oracle.rs').read_bytes()),performance_qualified=False,outcomes={})
try:
 for role in ['candidate','mutant']:
  for f in files:f.write_bytes((candidate/('candidate-'+f.name)).read_bytes())
  if role=='mutant':
   s=files[1].read_text();old='            self.collection_mut(collection)[idx] = row;\n            if collection == "docs" { self.docs_title_packed.take(); }\n            self.rebuild_row_maps_collection(collection);';new='            if collection == "docs" { self.docs_title_packed.take(); }\n            self.rebuild_row_maps_collection(collection);\n            self.collection_mut(collection)[idx] = row;';assert s.count(old)==1;files[1].write_text(s.replace(old,new))
  files[1].write_text(files[1].read_text()+'\n'+(p/'oracle.rs').read_text())
  with(p/(role+'.log')).open('w')as log:r=subprocess.run(['cargo','test','--release','--lib','store::incremental_rollback_fallback_oracle','--','--nocapture'],stdout=log,stderr=subprocess.STDOUT)
  text=(p/(role+'.log')).read_text();m['outcomes'][role]={'exit_code':r.returncode};assert (r.returncode==0)==(role=='candidate')
  if role=='candidate':assert '3 passed; 0 failed'in text
  else:assert 'panicked at'in text and 'test result: FAILED'in text and 'error[E'not in text
finally:
 for f,data in before.items():f.write_bytes(data)
 m['source_restored']=all(h(Path(f).read_bytes())==v for f,v in m['source_sha256'].items());(p/'provenance.json').write_text(json.dumps(m,indent=2)+'\n')
