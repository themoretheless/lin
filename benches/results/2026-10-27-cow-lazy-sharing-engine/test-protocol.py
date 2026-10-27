from pathlib import Path
import tempfile,json,shutil,subprocess,hashlib
src=Path(__file__).resolve().parent
with tempfile.TemporaryDirectory()as tmp:
 p=Path(tmp)
 for n in ['summarize.py','verify.py','cases.json']:shutil.copy2(src/n,p/n)
 def write(n,v):(p/n).write_text(json.dumps(v))
 cases=json.loads((p/'cases.json').read_text());records=[]
 for i in range(1,13):
  for m in ['baseline','control','candidate']:
   for case in cases:
    name=case.replace('/','_');base=p/f'{i:02}-{m}';(base/name).mkdir(parents=True)
    value=100 if m!='candidate'else 80
    d=dict(status='complete',cases=[case],observations=[dict(case=case,metric='wall',operations=1,value=value)]*8)
    (base/name/'run.json').write_text(json.dumps(d));(base/(name+'-resource.txt')).write_text('Maximum resident set size (kbytes): 123\n')
    records.append(dict(round=i,mode=m,case=case,exit_code=0,binary_sha256='c'if m=='candidate'else'b',glibc_tunables=None,peak_rss_kib=123))
 write('process-status.json',records);write('status.json',[dict(exit_code=0,subprocesses=37)]*36);write('test-status.json',dict(exit_code=0));write('provenance.json',dict(instrumentation=False,source_restored=True,source_sha256={},original_compare_sha256=hashlib.sha256(Path('benches/compare.rs').read_bytes()).hexdigest(),binaries={'baseline':{'sha256':'b'},'candidate':{'sha256':'c'}}))
 write('environment.json',dict(runtime_unchanged_all_roles=True,allocator_environment={'GLIBC_TUNABLES':None}));key='glibc.malloc.tcache_unsorted_limit';write('effective-tunables.json',{m:dict(glibc_tunables=None,loader_reported_malloc={key:16 if m=='candidate'else 0})for m in ['baseline','control','candidate']})
 subprocess.run(['python3',str(p/'summarize.py')],check=True);subprocess.run(['python3',str(p/'verify.py')],check=True)
 records[0]['peak_rss_kib']=124;write('process-status.json',records)
 bad=subprocess.run(['python3',str(p/'verify.py')],capture_output=True);assert bad.returncode!=0
 print('Synthetic complete matrix accepted; altered RSS record rejected')
