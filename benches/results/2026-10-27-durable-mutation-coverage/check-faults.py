from pathlib import Path
import hashlib,json,os,subprocess
p=Path(__file__).resolve().parent;m=json.loads((p/'provenance-fault.json').read_text());b=m['binary'];assert hashlib.sha256(Path(b['path']).read_bytes()).hexdigest()==b['sha256']
env=dict(os.environ,LIN_BENCH_SKIP_PG='1',LIN_BENCH_SKIP_MYSQL='1',RUST_BACKTRACE='0');env.pop('LIN_BENCH_MIXED_CASE',None);status=[]
for size in ['1k','10k']:
 for op in ['update','delete','commit','rollback']:
  for peer in ['lin','sqlite_delete','sqlite_wal']:
   case=f'compare/durable_{op}_1row_{size}/{peer}';dest=p/'fault-check'/case.replace('/','_');dest.parent.mkdir(exist_ok=True)
   proc=subprocess.Popen([b['path'],'--filter',case,'--samples','1','--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output',str(dest)],env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
   digest=hashlib.sha256();prefix=bytearray();count=0
   while chunk:=proc.stdout.read(65536):
    digest.update(chunk);count+=len(chunk)
    if len(prefix)<8192:prefix.extend(chunk[:8192-len(prefix)])
   code=proc.wait();dest.with_suffix('.prefix.log').write_bytes(prefix)
   assert code!=0,(case,'fault incorrectly accepted')
   assert b'full Lin values after reopen' in prefix or b'full SQLite values after reopen' in prefix,(case,'unexpected error',prefix[:200])
   status.append(dict(case=case,exit_code=code,output_bytes=count,output_sha256=digest.hexdigest(),prefix_only=True));(p/'fault-status.json').write_text(json.dumps(dict(all_faults_rejected=True,runs=status),indent=2)+'\n');print(case,'REJECTED',flush=True)
assert len(status)==24
