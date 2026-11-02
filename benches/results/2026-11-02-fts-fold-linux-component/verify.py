from pathlib import Path
import hashlib,json,math,statistics,sys
p=Path(__file__).resolve().parent;root=p.parents[2];series=p/sys.argv[1];sha=lambda b:hashlib.sha256(b).hexdigest()
inputs=json.loads((p/'input.json').read_text());build=json.loads((p/'build-metadata.json').read_text());status=json.loads((series/'measure-status.json').read_text());obs=json.loads((series/'observations.json').read_text());summary=json.loads((series/'summary.json').read_text());manifest=json.loads((series/'raw-manifest.json').read_text())
for name,h in inputs['files_sha256'].items():assert sha((root/name).read_bytes())==h,name
assert status['complete'] is True and status['peer_comparisons']==0 and status['processes']==54 and status['samples']==432 and status['exact_rows_verified']==1998000
assert '1.96.0' in status['rustc'] and 'x86_64-unknown-linux-gnu' in status['rustc'] and status['rustc']==build['rustc']
assert build['features']==['default','experimental-extent-wal']
assert build['commands']==[dict(role=role,command=['cargo',action,'--release','-p','lin-peer-worker','--features','lin/experimental-extent-wal'])for role in ['base','cand']for action in ['test','build']]
for role in ['base','cand']:
 log=(p/(role+'-test.log')).read_text()
 assert '1 passed; 0 failed; 0 ignored;'in log and 'hundred_thousand_native_rows_respect_default_statement_quotas ... ok'in log
assert build['runtime_reference']==inputs['runtime_reference']
for name,h in status['source_sha256'].items():assert inputs['files_sha256'][name]==h,name
for role in ['base','cand']:
 b=build['binaries'][role];assert status['binaries_sha256'][b['path']]==b['sha256']==sha(Path(b['path']).read_bytes())
sys.path.insert(0,str(root/'scripts'));from benchmark_role_balance import role_order
expected=[(r,n,role)for r in range(1,7)for ci,n in enumerate([1000,10000,100000])for role in role_order(r,ci,['base','cont','cand'])]
assert len(obs)==54 and [(o['round'],o['rows'],o['role'])for o in obs]==expected
assert [(c['round'],c['rows'],c['role'])for c in status['commands']]==expected
assert set(manifest)=={f'{r}-{n}-{role}.{ext}'for r,n,role in expected for ext in ['json','log']}
for name,h in manifest.items():assert sha((series/'raw'/name).read_bytes())==h,name
checked=0
for o,c in zip(obs,status['commands']):
 r,n,role=o['round'],o['rows'],o['role'];assert c['command']==[build['binaries']['cand'if role=='cand'else'base']['path']];assert c['request']==dict(insert_rows=n,samples=8,timestamp_ms=1700000000000)
 d=json.loads((series/'raw'/f'{r}-{n}-{role}.json').read_text());v=d['samples_ns'];assert len(v)==8 and all(type(x)in(int,float)and math.isfinite(x)and x>0 for x in v);assert v==o['samples_ns'] and statistics.median(v)==o['median_ns'];assert len(d['result'])==n and d['statements']==(n+9999)//10000 and d['statement_rows_limit']==10000
 for i,row in enumerate(d['result']):assert row==[f'd-{i}',f'bench://{i}','rag'if i%2==0 else'sys',f'doc {i} wal note'if i%10==0 else f'doc {i} plain',1700000000000,f'body {i}']
 checked+=n
for n in [1000,10000,100000]:
 gains=[];aa=[]
 for r in range(1,7):
  v={o['role']:o['median_ns']for o in obs if o['round']==r and o['rows']==n};gains.append((1-v['cand']/v['base'])*100);aa.append(abs(v['cont']/v['base']-1)*100)
 assert summary[str(n)]==dict(median_gain_percent=statistics.median(gains),median_aa_percent=statistics.median(aa),positive_pairs=sum(x>0 for x in gains),gains_percent=gains)
assert checked==1998000
(series/'independent-audit.json').write_text(json.dumps(dict(processes=54,samples=432,exact_rows_verified=checked,raw_members_verified=108,schedule_commands_binary_sources_and_summary_verified=True,performance_qualified=False,peer_comparisons=0),indent=2)+'\n');print('Independent series audit passed; no adoption or peer qualification')
