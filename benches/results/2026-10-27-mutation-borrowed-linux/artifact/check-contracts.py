from pathlib import Path
import subprocess,json
p=Path(__file__).resolve().parent;names=['exec','store'];before={n:Path(f'src/{n}.rs').read_bytes()for n in names};results={}
try:
 for mode,folder in [('rejected',p.parent/'2026-10-26-mutation-row-copy-engine'),('candidate',p)]:
  for n in names:Path(f'src/{n}.rs').write_bytes((folder/f'candidate-{n}.rs').read_bytes())
  with (p/f'contracts-{mode}.log').open('w')as log:r=subprocess.run(['cargo','test','--test','mutation_contract'],stdout=log,stderr=subprocess.STDOUT)
  results[mode]=r.returncode
finally:
 for n in names:Path(f'src/{n}.rs').write_bytes(before[n])
 (p/'contract-status.json').write_text(json.dumps(results,indent=2)+'\n')
assert results=={'rejected':101,'candidate':0},results
print('rejected candidate fails regressions, revised candidate passes; source restored',flush=True)
