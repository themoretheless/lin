from pathlib import Path
import subprocess,json,hashlib,re,shutil
p=Path(__file__).resolve().parent;f=Path('src/persist.rs');before=f.read_bytes();h=lambda data:hashlib.sha256(data).hexdigest();meta={'source_commit':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'source_sha256':{str(s):h(s.read_bytes())for s in Path('src').rglob('*.rs')},'helper_sha256':h((p/'helper.rs').read_bytes()),'oracle_sha256':h((p/'io-oracle.rs').read_bytes()),'performance_qualified':False}
try:
 f.write_bytes(before+b'\n'+(p/'helper.rs').read_bytes()+b'\n'+(p/'io-oracle.rs').read_bytes())
 with(p/'io-build.log').open('w')as log:r=subprocess.run(['cargo','test','--release','--lib','persist::single_buffer_oracle','--','--nocapture'],stdout=log,stderr=subprocess.STDOUT)
 assert r.returncode==0;source=re.search(r'Running unittests src/lib.rs \(([^)]+)\)',(p/'io-build.log').read_text())[1];binary=Path('/tmp/lin-single-buffer-io-tests');shutil.copy2(source,binary);meta['binary']={'path':str(binary),'sha256':h(binary.read_bytes())}
finally:
 f.write_bytes(before);meta['source_restored']=all(h(Path(s).read_bytes())==v for s,v in meta['source_sha256'].items());(p/'io-provenance.json').write_text(json.dumps(meta,indent=2)+'\n')
