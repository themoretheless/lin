from pathlib import Path
import subprocess,json,hashlib,platform,os
p=Path(__file__).resolve().parent
assert platform.system()=='Linux' and platform.machine()=='x86_64'
root=p/'measurement';root.mkdir(exist_ok=False)
h=lambda f:hashlib.sha256(Path(f).read_bytes()).hexdigest()
binary=p/'probe';subprocess.run(['rustc','-O',str(p/'probe.rs'),'-o',str(binary)],check=True)
(root/'environment.json').write_text(json.dumps(dict(platform=platform.platform(),rustc=subprocess.check_output(['rustc','-Vv'],text=True),filesystem=subprocess.check_output(['stat','-f',str(root)],text=True),mount=subprocess.check_output(['findmnt','-T',str(root),'-J'],text=True),source_sha256=h(p/'probe.rs'),binary_sha256=h(binary)),indent=2)+'\n')
cases=[(size,offset)for size in [117,4095,4096,4097,4125,8192]for offset in [0,2048,4095]]
records=[]
for repeat in range(1,7):
 ordered=cases[repeat:]+cases[:repeat]
 for i,(size,offset)in enumerate(ordered):
  roles=['original','control'] if (repeat+i)%2==0 else ['control','original']
  for role in roles:
   name=f'{repeat}-{size}-{offset}-{role}';cmd=[str(binary),str(root/name),str(size),str(offset),'8']
   assert h(binary)==json.loads((root/'environment.json').read_text())['binary_sha256']
   r=subprocess.run(cmd,text=True,capture_output=True);(root/(name+'.jsonl')).write_text(r.stdout);(root/(name+'.stderr')).write_text(r.stderr)
   records.append(dict(round=repeat,size=size,offset=offset,role=role,command=cmd,exit_code=r.returncode,raw_sha256=h(root/(name+'.jsonl'))));(root/'process-status.json').write_text(json.dumps(records,indent=2)+'\n');assert r.returncode==0,name
 print(repeat,'complete',flush=True)
assert len(records)==216
