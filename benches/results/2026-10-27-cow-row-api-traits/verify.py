from pathlib import Path
import subprocess,json,hashlib
p=Path(__file__).resolve().parent;frozen=p.parent/'2026-10-27-cow-case-isolated-gate';names=['store','exec','check'];before={n:Path(f'src/{n}.rs').read_bytes()for n in names};example=Path('examples/row_api_traits.rs');assert not example.exists();statuses=[];hashes={}
try:
 example.write_bytes((p/'gate.rs').read_bytes())
 for mode in ['baseline','frozen_cow','traits_fixed_cow']:
  for n in names:
   b=before[n]if mode=='baseline'else(frozen/f'candidate-{n}.rs').read_bytes()
   if mode=='traits_fixed_cow'and n=='store':
    s=b.decode();needle='#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]\n#[serde(transparent)]\npub struct Row';assert s.count(needle)==1;s=s.replace(needle,'#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]\n#[serde(transparent)]\npub struct Row');b=s.encode();(p/'candidate-store.rs').write_bytes(b)
   Path(f'src/{n}.rs').write_bytes(b);hashes[f'{mode}/{n}']=hashlib.sha256(b).hexdigest()
  with(p/f'{mode}.log').open('w')as log:r=subprocess.run(['cargo','run','--example','row_api_traits'],stdout=log,stderr=subprocess.STDOUT)
  statuses.append({'mode':mode,'exit_code':r.returncode})
  if mode=='frozen_cow':
   text=(p/f'{mode}.log').read_text();assert r.returncode==101 and 'E0277'in text and 'Hash'in text and 'Eq'in text and 'not satisfied'in text
  else:assert r.returncode==0
finally:
 for n,b in before.items():Path(f'src/{n}.rs').write_bytes(b)
 example.unlink(missing_ok=True);(p/'status.json').write_text(json.dumps(statuses,indent=2)+'\n');(p/'source-sha256.json').write_text(json.dumps(hashes,indent=2)+'\n')
