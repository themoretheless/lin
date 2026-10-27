from pathlib import Path
import subprocess,json,hashlib
p=Path(__file__).resolve().parent;files=[Path('src/exec.rs'),Path('src/store.rs')];before={f:f.read_bytes()for f in files};test=Path('tests/clone_free_owned_rollback.rs');assert not test.exists();h=lambda b:hashlib.sha256(b).hexdigest();m={'source_commit':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'source_sha256':{str(f):h(f.read_bytes())for f in Path('src').rglob('*.rs')},'performance_qualified':False}
try:
 exec=before[files[0]].decode();needle='self.store.restore_rows(&[(collection, idx, row)])';assert exec.count(needle)==2;exec=exec.replace(needle,'self.store.restore_row_owned_no_clone(&collection, idx, row)');store=before[files[1]].decode();needle='    pub fn restore_rows(&mut self, journal: &[(String, usize, Row)]) {';assert store.count(needle)==1;store=store.replace(needle,(p/'helper.rs').read_text()+needle);(p/'candidate-exec.rs').write_text(exec);(p/'candidate-store.rs').write_text(store);files[0].write_text(exec);files[1].write_text(store);test.write_bytes((p/'regression.rs').read_bytes())
 with(p/'full-tests.log').open('w')as log:r=subprocess.run(['cargo','test','--release','--lib','--tests','--','--nocapture'],stdout=log,stderr=subprocess.STDOUT)
 m['exit_code']=r.returncode;assert r.returncode==0,'inspect full-tests.log'
finally:
 for f,data in before.items():f.write_bytes(data)
 test.unlink(missing_ok=True);m['source_restored']=all(h(Path(f).read_bytes())==v for f,v in m['source_sha256'].items());(p/'test-status.json').write_text(json.dumps(m,indent=2)+'\n')
