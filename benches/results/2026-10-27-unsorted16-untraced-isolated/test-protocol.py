from pathlib import Path
import tempfile,json,shutil,subprocess
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
    records.append(dict(round=i,mode=m,case=case,exit_code=0,glibc_tunables='x'if m=='candidate'else None,peak_rss_kib=123))
 write('process-status.json',records);write('status.json',[dict(exit_code=0,subprocesses=28)]*36);write('test-status.json',dict(exit_code=0));write('provenance.json',dict(instrumentation=False,one_binary_all_roles=True,source_sha256={}))
 key='glibc.malloc.tcache_unsorted_limit';write('effective-tunables.json',{m:dict(glibc_tunables='x'if m=='candidate'else None,loader_reported_malloc={key:16 if m=='candidate'else 0})for m in ['baseline','control','candidate']})
 subprocess.run(['python3',str(p/'summarize.py')],check=True);subprocess.run(['python3',str(p/'verify.py')],check=True)
 records[0]['peak_rss_kib']=124;write('process-status.json',records)
 bad=subprocess.run(['python3',str(p/'verify.py')],capture_output=True);assert bad.returncode!=0
 print('Synthetic complete matrix accepted; altered RSS record rejected')
