from pathlib import Path
import subprocess,json,hashlib,shutil
p=Path(__file__).resolve().parent;example=Path('examples/cow_lifecycle_memory.rs');assert not example.exists();names=['store','exec','check'];before={n:Path(f'src/{n}.rs').read_bytes() for n in names};statuses=[];meta={'source_commit':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'before_sha256':{n:hashlib.sha256(b).hexdigest()for n,b in before.items()},'candidate_sha256':{}}
try:
 example.write_bytes((p/'gate.rs').read_bytes())
 for mode in ['baseline','candidate']:
  for n in names:
   b=before[n] if mode=='baseline' else(p.parent/'2026-10-27-cow-inplace-lock-fixed-gate'/f'candidate-{n}.rs').read_bytes();Path(f'src/{n}.rs').write_bytes(b)
   if mode=='candidate':meta['candidate_sha256'][n]=hashlib.sha256(b).hexdigest()
  with(p/f'build-{mode}.log').open('w')as log:r=subprocess.run(['cargo','build','--example','cow_lifecycle_memory'],stdout=log,stderr=subprocess.STDOUT)
  assert r.returncode==0
  binary=Path(f'/tmp/lin-cow-lifecycle-{mode}');shutil.copy2('target/debug/examples/cow_lifecycle_memory',binary);meta[mode+'_binary_sha256']=hashlib.sha256(binary.read_bytes()).hexdigest()
 for process in range(1,4):
  for n in [1000,10000,100000]:
   for mode in (['baseline','candidate'] if process%2 else ['candidate','baseline']):
    with(p/f'{mode}-{n}-{process}.jsonl').open('w')as out,(p/f'{mode}-{n}-{process}.log').open('w')as log:r=subprocess.run([f'/tmp/lin-cow-lifecycle-{mode}',str(n)],stdout=out,stderr=log)
    statuses.append({'mode':mode,'rows':n,'process':process,'exit_code':r.returncode});assert r.returncode==0
finally:
 for n,b in before.items():Path(f'src/{n}.rs').write_bytes(b)
 example.unlink(missing_ok=True);(p/'status.json').write_text(json.dumps(statuses,indent=2)+'\n');(p/'provenance.json').write_text(json.dumps(meta,indent=2)+'\n')
