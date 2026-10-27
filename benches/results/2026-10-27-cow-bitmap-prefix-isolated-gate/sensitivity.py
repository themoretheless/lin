from pathlib import Path
import subprocess,json
p=Path(__file__).resolve().parent;names=['store','exec','check','index'];before={n:Path(f'src/{n}.rs').read_bytes()for n in names};status=[]
try:
 for mode in ['nonempty_reported_empty','allocated_zero_word_reported_nonempty']:
  for n in names:Path(f'src/{n}.rs').write_bytes((p/f'candidate-{n}.rs').read_bytes())
  f=Path('src/index.rs');s=f.read_text();needle='if set.bits.first().is_some_and(|word| *word != 0) {\n                    false';assert s.count(needle)==1
  if mode=='nonempty_reported_empty':s=s.replace(needle,needle[:-5]+'true',1)
  else:s=s.replace('set.bits.first().is_some_and(|word| *word != 0)','set.bits.first().is_some_and(|_word| true)',1)
  f.write_text(s+'\n'+(p/'index-oracle.rs').read_text())
  with(p/f'mutant-{mode}.log').open('w')as log:r=subprocess.run(['cargo','test','--lib','bitmap_empty_oracle'],stdout=log,stderr=subprocess.STDOUT)
  status.append({'variant':mode,'exit_code':r.returncode});assert r.returncode==101
  text=(p/f'mutant-{mode}.log').read_text();assert 'test result: FAILED.'in text and 'assertion'in text
finally:
 for n,b in before.items():Path(f'src/{n}.rs').write_bytes(b)
 (p/'bitmap-sensitivity-status.json').write_text(json.dumps(status,indent=2)+'\n')
