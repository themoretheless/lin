from pathlib import Path
import hashlib,json,subprocess,sys,tempfile
p=Path(__file__).resolve().parent
with tempfile.TemporaryDirectory(prefix='lin-durable-protocol-')as tmp:
 t=Path(tmp);dump=lambda path,obj:path.write_text(json.dumps(obj)+'\n');cases=json.loads((p/'cases.json').read_text());dump(t/'cases.json',cases)
 source=t/'source.rs';source.write_text('fixture');digest=hashlib.sha256(source.read_bytes()).hexdigest();dump(t/'provenance.json',dict(source_restored=True,instrumentation=False,source_sha256={str(source):digest},original_compare_path=str(source),original_compare_sha256=digest,binary=dict(sha256='a'*64)))
 dump(t/'environment.json',dict(runtime_unchanged_all_roles=True,allocator_environment=dict(GLIBC_TUNABLES=None)));records=[];status=[]
 for i in range(1,13):
  for mode in ['baseline','control']:
   for index,c in enumerate(cases):
    base=t/f'{i:02}-{mode}';base.mkdir(exist_ok=True);name=c.replace('/','_');dest=base/name;dest.mkdir()
    value=(100 if c.endswith('/lin')else 200 if c.endswith('/sqlite_delete')else 80)+i+(mode=='control')+index
    dump(dest/'run.json',dict(status='complete',cases=[dict(id=c)],observations=[dict(case=c,metric='wall',operations=1,value=str(value+j))for j in range(8)]))
    (base/(name+'-resource.txt')).write_text('Maximum resident set size (kbytes): 2048\n')
    records.append(dict(round=i,mode=mode,case=c,exit_code=0,peak_rss_kib=2048,binary_sha256='a'*64,glibc_tunables=None))
   status.append(dict(run=f'{i:02}-{mode}',exit_code=0,subprocesses=24))
 dump(t/'process-status.json',records);dump(t/'status.json',status)
 subprocess.run([sys.executable,str(p/'analyze.py'),str(t)],check=True);subprocess.run([sys.executable,str(p/'verify.py'),str(t)],check=True)
 resource=t/'01-baseline'/(cases[0].replace('/','_')+'-resource.txt');original=resource.read_text();resource.write_text(original.replace('2048','2049'))
 assert subprocess.run([sys.executable,str(p/'verify.py'),str(t)],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL).returncode!=0
 resource.write_text(original)
 report=json.loads((t/'summary.json').read_text());report['comparisons'][0]['lin_wins']=99;dump(t/'summary.json',report)
 assert subprocess.run([sys.executable,str(p/'verify.py'),str(t)],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL).returncode!=0
 print('PASS: full synthetic matrix accepted; altered RSS and peer win count rejected')
