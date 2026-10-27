from pathlib import Path
import subprocess,json
p=Path(__file__).resolve().parent;f=Path('src/index.rs');before=f.read_bytes();status=[]
try:
 for limit in [0,1,2,16]:
  s=(p/'candidate-index.rs').read_text().replace('const PACKED_MAX_WORDS: usize = (u32::MAX as usize) / 64;',f'const PACKED_MAX_WORDS: usize = {limit};');f.write_text(s+'\n'+(p/'transition-oracle.rs').read_text())
  with(p/f'transition-{limit}.log').open('w')as log:r=subprocess.run(['cargo','test','--lib','explicit_packed_boundary'],stdout=log,stderr=subprocess.STDOUT)
  assert r.returncode==0;status.append(dict(limit=limit,exit_code=r.returncode));print(limit,'packed/fallback transitions passed',flush=True)
finally:
 f.write_bytes(before);(p/'transition-status.json').write_text(json.dumps(dict(runs=status,source_restored=f.read_bytes()==before),indent=2)+'\n')
