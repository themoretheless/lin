from pathlib import Path
import hashlib,json,subprocess
p=Path(__file__).resolve().parent
files=[Path('src/persist.rs'),Path('src/store.rs')];before={f:f.read_bytes()for f in files};h=lambda b:hashlib.sha256(b).hexdigest();m=dict(source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),source_sha256={str(f):h(f.read_bytes())for f in Path('src').rglob('*.rs')},performance_qualified=False)
try:
 persist=before[files[0]].decode()+'\n'+(p/'helper.rs').read_text();store=before[files[1]].decode();needle='None => persist::append_record(log, &rec, encode_buf, sync, log_bytes)?,';assert store.count(needle)==1;store=store.replace(needle,needle.replace('append_record(','append_record_buffered('));(p/'candidate-persist.rs').write_text(persist);(p/'candidate-store.rs').write_text(store);files[0].write_text(persist+'\n'+(p/'oracle.rs').read_text());files[1].write_text(store)
 with(p/'full-tests.log').open('w')as log:r=subprocess.run(['cargo','test','--release','--lib','--tests','--','--nocapture'],stdout=log,stderr=subprocess.STDOUT)
 m['exit_code']=r.returncode;assert r.returncode==0,'inspect full-tests.log'
finally:
 for f,data in before.items():f.write_bytes(data)
 m['source_restored']=all(h(Path(f).read_bytes())==v for f,v in m['source_sha256'].items());(p/'test-status.json').write_text(json.dumps(m,indent=2)+'\n')
