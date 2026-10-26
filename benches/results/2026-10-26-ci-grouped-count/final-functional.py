from pathlib import Path
import subprocess,json,hashlib,re
p=Path(__file__).resolve().parent;out={}
for name,args in [('final-workspace',['cargo','test','--workspace']),('no-default',['cargo','check','--lib','--no-default-features']),('optional',['cargo','check','--lib','--features','gpu,parallel,compress-wal,compress-lz4,compress-flate']),('final-release',['cargo','bench','--bench','compare','--no-run'])]:
 with (p/(name+'.log')).open('w') as log:r=subprocess.run(args,stdout=log,stderr=subprocess.STDOUT)
 out[name]=r.returncode;(p/'final-functional.json').write_text(json.dumps(out,indent=2));print(name,r.returncode,flush=True)
 assert r.returncode==0
exe=re.search(r'Executable benches/compare.rs \(([^)]+)\)',(p/'final-release.log').read_text())[1]
sha=lambda f:hashlib.sha256(Path(f).read_bytes()).hexdigest()
meta=json.loads((p/'provenance.json').read_text());assert sha(exe)==meta['candidate']['binary_sha256']
out['final_binary_equal_measured']=True;(p/'final-functional.json').write_text(json.dumps(out,indent=2))
