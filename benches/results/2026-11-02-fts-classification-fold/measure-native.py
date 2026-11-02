from pathlib import Path
import hashlib,json,math,os,platform,statistics,subprocess,sys
p=Path(__file__).resolve().parent;root=p.parents[2];sha=lambda b:hashlib.sha256(b).hexdigest();build=json.loads((p/'worker-build.json').read_text());assert not(p/'native-process-status.json').exists()and not(p/'native-raw').exists(),'preserve previous attempt'
sys.path.insert(0,str(root/'scripts'));from benchmark_source_integrity import snapshot
from benchmark_role_balance import role_order,validate_orders
assert snapshot(root,None)['files_sha256']==build['candidate']['source_sha256']
for role in ['baseline','candidate']:assert sha(Path(build[role]['path']).read_bytes())==build[role]['sha256']
(p/'native-raw').mkdir();sizes=[1000,10000,100000];roles=['base','cont','cand'];orders={str(n):[role_order(r,ci,roles)for r in range(1,7)]for ci,n in enumerate(sizes)};validate_orders(orders,roles);raw=[]
for series in [1,2]:
 for rnd in range(1,7):
  for n in sizes:
   for role in orders[str(n)][rnd-1]:
    binary=build['candidate'if role=='cand'else'baseline'];command=[binary['path']];request=dict(insert_rows=n,samples=8,timestamp_ms=1700000000000);r=subprocess.run(command,input=json.dumps(request)+'\n',text=True,capture_output=True,cwd=root);name=f'{series}-{rnd}-{n}-{role}';(p/'native-raw'/(name+'.json')).write_text(r.stdout);(p/'native-raw'/(name+'.log')).write_text(r.stderr);assert r.returncode==0
    d=json.loads(r.stdout);v=d['samples_ns'];assert len(v)==8 and all(type(x)in(int,float)and math.isfinite(x)and x>0 for x in v);assert len(d['result'])==n and d['statements']==(n+9999)//10000 and d['statement_rows_limit']==10000
    for i,row in enumerate(d['result']):assert row==[f'd-{i}',f'bench://{i}','rag'if i%2==0 else'sys',f'doc {i} wal note'if i%10==0 else f'doc {i} plain',1700000000000,f'body {i}']
    raw.append(dict(series=series,round=rnd,rows=n,role=role,command=command,request=request,samples_ns=v,median_ns=statistics.median(v),binary_sha256=binary['sha256']));(p/'native-process-status.json').write_text(json.dumps(raw,indent=2)+'\n')
assert snapshot(root,None)['files_sha256']==build['candidate']['source_sha256']
for role in ['baseline','candidate']:assert sha(Path(build[role]['path']).read_bytes())==build[role]['sha256']
summary={}
for series in [1,2]:
 for n in sizes:
  gains=[];aa=[]
  for rnd in range(1,7):
   v={o['role']:o['median_ns']for o in raw if o['series']==series and o['rows']==n and o['round']==rnd};gains.append((1-v['cand']/v['base'])*100);aa.append(abs(v['cont']/v['base']-1)*100)
  summary[f'{series}-{n}']=dict(median_gain_percent=statistics.median(gains),median_aa_percent=statistics.median(aa),positive_pairs=sum(x>0 for x in gains),gains_percent=gains)
(p/'native-summary.json').write_text(json.dumps(summary,indent=2)+'\n');(p/'native-status.json').write_text(json.dumps(dict(complete=True,processes=108,samples=864,rows_all_six_fields_verified=3996000,source_and_binaries_unchanged=True,compiler=build['compiler'],features=build['features'],platform=platform.platform(),external_peer_comparisons=0,performance_qualified=False),indent=2)+'\n');print(json.dumps(summary,indent=2))
