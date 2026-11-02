from pathlib import Path
import json,subprocess,os,hashlib,ast
p=Path(__file__).resolve().parent;m=json.loads((p/'provenance.json').read_text());cases=json.loads((p/'cases.json').read_text())
assert m['source_restored'] and not m['instrumentation'] and len(cases)==len(set(cases))==20
for n,h in m['source_sha256'].items():assert hashlib.sha256((p.parents[2]/n).read_bytes()).hexdigest()==h
dest=p/'qualification-smoke';dest.mkdir(exist_ok=False);records=[]
for role in ['baseline','candidate']:
 binary=m['binaries'][role];assert hashlib.sha256(Path(binary['path']).read_bytes()).hexdigest()==binary['sha256']
 for case in cases:
  out=dest/(role+'-'+case.replace('/','_'));cmd=[binary['path'],'--filter',case,'--samples','1','--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output',str(out)]
  with out.with_suffix('.log').open('w')as log:r=subprocess.run(cmd,stdout=log,stderr=subprocess.STDOUT,env={k:v for k,v in dict(os.environ,LIN_BENCH_SKIP_MYSQL='1',LIN_BENCH_SKIP_PG='1',LC_ALL='C').items()if k not in ['LIN_BENCH_MIXED_CASE','LIN_COLUMN_BATCH_FIXTURE']})
  records.append(dict(role=role,case=case,exit=r.returncode));(p/'qualification-smoke-status.json').write_text(json.dumps(records,indent=2)+'\n');assert r.returncode==0
  q=json.loads((out/'run.json').read_text());assert q['status']=='complete'and len(q['observations'])==1 and q['cases'][0]['id']==case
assert len(records)==40

for n,h in m["source_sha256"].items():assert hashlib.sha256((p.parents[2]/n).read_bytes()).hexdigest()==h
print("All40 fresh paired functional checks passed; no performance qualification")
