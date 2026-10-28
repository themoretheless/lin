from pathlib import Path
import subprocess,json,hashlib,os,shutil,re
p=Path(__file__).resolve().parent;target=Path('/tmp/lin-cow-vnext-target');f=Path('src/index.rs');before=f.read_bytes();hashes={str(x):hashlib.sha256(x.read_bytes()).hexdigest()for x in Path('src').rglob('*.rs')};records=[]
try:
 for role,ref in [('baseline','a7d4b02c3'),('allocation_free','884ebeb81')]:
  data=subprocess.check_output(['git','show',ref+':src/index.rs']);f.write_bytes(data);known={str(x):x.stat().st_mtime_ns for x in target.rglob('*.ll')}
  with(p/('compile-'+role+'.log')).open('w')as log:r=subprocess.run(['cargo','rustc','--release','--lib','--color','never','--','--emit=llvm-ir'],env=dict(os.environ,CARGO_TARGET_DIR=str(target)),stdout=log,stderr=subprocess.STDOUT)
  assert r.returncode==0,role
  changed=[x for x in target.rglob('*.ll')if known.get(str(x))!=x.stat().st_mtime_ns];assert changed,role;dest=p/role;dest.mkdir(exist_ok=False);files=[];definitions=[];calls=[]
  for i,path in enumerate(sorted(changed)):
   data=path.read_bytes();out=dest/f'{i:02}-{path.name}';out.write_bytes(data);files.append(dict(source_path=str(path),saved_path=str(out.relative_to(p)),sha256=hashlib.sha256(data).hexdigest()))
   for line in data.decode().splitlines():
    if 'index_covers_pred'in line:
     if line.startswith('define '):definitions.append(line)
     if re.search(r'\b(?:call|invoke)\b',line):calls.append(line.strip())
  records.append(dict(role=role,source_reference=ref,index_sha256=hashlib.sha256(f.read_bytes()).hexdigest(),files=files,definitions=definitions,direct_call_lines=calls,performance_qualified=False));(p/'codegen.json').write_text(json.dumps(records,indent=2)+'\n');print(role,'IR files',len(files),'definitions',len(definitions),'direct calls',len(calls),flush=True)
finally:
 f.write_bytes(before);restored=all(hashlib.sha256(Path(x).read_bytes()).hexdigest()==h for x,h in hashes.items());(p/'status.json').write_text(json.dumps(dict(all_sources_restored=restored,source_sha256=hashes,compiler=subprocess.check_output(['rustc','-Vv'],text=True),command=['cargo','rustc','--release','--lib','--','--emit=llvm-ir'],scope='local optimized library IR, not final Linux benchmark code or timing',performance_qualified=False),indent=2)+'\n')
assert restored
