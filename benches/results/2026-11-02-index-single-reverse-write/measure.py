from pathlib import Path
import hashlib,json,math,os,platform,shutil,statistics,subprocess,sys
p=Path(__file__).resolve().parent;root=p.parents[2];sha=lambda b:hashlib.sha256(b).hexdigest()
assert not(p/'raw.json').exists(),'preserve earlier attempt'
artifacts=[]
for line in(p/'build.log').read_text().splitlines():
 try:d=json.loads(line)
 except ValueError:continue
 if d.get('reason')=='compiler-artifact'and d.get('target',{}).get('name')=='lin'and d.get('profile',{}).get('test')and d.get('executable'):artifacts.append(d['executable'])
assert len(artifacts)==1
binary=Path('/tmp/lin-index-single-reverse-component');assert not binary.exists();shutil.copy2(artifacts[0],binary);identity=sha(binary.read_bytes());source=sha((root/'src/index.rs').read_bytes());sys.path.insert(0,str(root/'scripts'));from benchmark_role_balance import role_order,validate_orders
sizes=[1000,10000,100000];roles=['base','cont','cand'];orders={str(n):[role_order(r,ci,roles)for r in range(1,7)]for ci,n in enumerate(sizes)};validate_orders(orders,roles);raw=[]
for series in [1,2]:
 for rnd in range(1,7):
  for ci,n in enumerate(sizes):
   for role in orders[str(n)][rnd-1]:
    command=[str(binary),'--ignored','--exact','index::single_reverse_write_component::measure_component','--nocapture'];env=dict(os.environ,LIN_INDEX_COMPONENT_ROLE=role,LIN_INDEX_COMPONENT_ROWS=str(n));r=subprocess.run(command,cwd=root,env=env,text=True,capture_output=True)
    name=f'{series}-{rnd}-{n}-{role}';(p/(name+'.log')).write_text(r.stdout+r.stderr);assert r.returncode==0
    lines=[json.loads(s)for s in r.stdout.splitlines()if s.startswith('{')];assert len(lines)==1;d=lines[0];assert d['role']==role and d['rows']==n and d['helper_calls_per_sample']==2000 and d['all_index_state_verified']is True and d['performance_qualified']is False
    values=d['samples_ns'];assert len(values)==8 and all(type(v)in(int,float)and math.isfinite(v)and v>0 for v in values);raw.append(dict(series=series,round=rnd,role=role,rows=n,command=command,env={k:env[k]for k in ['LIN_INDEX_COMPONENT_ROLE','LIN_INDEX_COMPONENT_ROWS']},samples_ns=values,median_ns=statistics.median(values)));(p/'process-status.json').write_text(json.dumps(raw,indent=2)+'\n')
assert sha(binary.read_bytes())==identity and sha((root/'src/index.rs').read_bytes())==source
summary={}
for series in [1,2]:
 for n in sizes:
  gains=[];aa=[]
  for rnd in range(1,7):
   v={o['role']:o['median_ns']for o in raw if o['series']==series and o['rows']==n and o['round']==rnd};gains.append((1-v['cand']/v['base'])*100);aa.append(abs(v['cont']/v['base']-1)*100)
  summary[f'{series}-{n}']=dict(median_gain_percent=statistics.median(gains),median_aa_percent=statistics.median(aa),positive_pairs=sum(v>0 for v in gains),gains_percent=gains)
(p/'raw.json').write_text(json.dumps(raw,indent=2)+'\n');(p/'summary.json').write_text(json.dumps(summary,indent=2)+'\n');(p/'status.json').write_text(json.dumps(dict(complete=True,component_only=True,performance_qualified=False,external_peer_comparisons=0,processes=108,samples=864,helper_calls_per_sample=2000,binary_sha256=identity,index_source_sha256=source,compiler=subprocess.check_output(['rustc','-Vv'],text=True),platform=platform.platform()),indent=2)+'\n');print(json.dumps(summary,indent=2))
