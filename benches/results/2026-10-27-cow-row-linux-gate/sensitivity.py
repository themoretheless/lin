from pathlib import Path
import subprocess,json
p=Path(__file__).resolve().parent;names=['store','exec','check'];before={n:Path(f'src/{n}.rs').read_bytes()for n in names};assert all(b==(p/f'before-{n}.rs').read_bytes()for n,b in before.items())
try:
 for n in names:Path(f'src/{n}.rs').write_bytes((p/f'candidate-{n}.rs').read_bytes())
 f=Path('src/store.rs');s=f.read_text();needle='#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]\n#[serde(transparent)]\npub struct Row';assert s.count(needle)==1
 s=s.replace(needle,'#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]\n#[serde(transparent)]\npub struct Row')
 s+='\nimpl Clone for Row { fn clone(&self)->Self { Self::default() } }\n'+(p/'oracle.rs').read_text();f.write_text(s)
 with(p/'mutant.log').open('w')as log:r=subprocess.run(['cargo','test','--lib','cow_row_oracle'],stdout=log,stderr=subprocess.STDOUT)
 assert r.returncode!=0 and 'test result: FAILED' in(p/'mutant.log').read_text()
 (p/'sensitivity-status.json').write_text(json.dumps({'lost_clone_payload_detected':True,'exit_code':r.returncode},indent=2)+'\n')
finally:
 for n,b in before.items():Path(f'src/{n}.rs').write_bytes(b)
print('COW oracle detects lost cloned fields; production source restored')
