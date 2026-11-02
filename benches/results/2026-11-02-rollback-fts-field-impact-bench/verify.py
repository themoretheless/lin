from pathlib import Path
import contextlib,hashlib,io,json,math,re,runpy,subprocess,sys
sys.path.insert(0,str(Path.cwd()/'scripts'))
from benchmark_role_balance import schedule,orders_from_records
HERE=Path(__file__).resolve().parent

def median(values):
 s=sorted(values);return(s[(len(s)-1)//2]+s[len(s)//2])/2

def verify(p,allow_synthetic=False):
 load=lambda name:json.loads((p/name).read_text());sha=lambda f:hashlib.sha256(Path(f).read_bytes()).hexdigest()
 env=load('environment.json');assert not env.get('synthetic_fixture') or allow_synthetic
 assert 'Linux' in env['platform'] and env['same_runtime_all_roles']
 for name,h in load('frozen.json').items():assert sha(name)==h,name
 with contextlib.redirect_stdout(io.StringIO()):runpy.run_path(str(HERE.parent/'2026-11-02-rollback-fts-field-impact/verify-functional.py'),run_name='__main__')
 m=load('input.json');meta=load('provenance.json')
 assert meta['source_restored'] and not meta['instrumentation']
 assert meta['source_sha256']==m['candidate']['source_sha256'] and all(sha(f)==h for f,h in meta['source_sha256'].items())
 assert sha('benches/compare.rs')==meta['original_compare_sha256'] and sha(p/'compare.rs')==meta['compiled_compare_sha256']
 for role in ['baseline','candidate']:
  info=m[role];binary=meta['binaries'][role]
  assert binary['source_sha256']==info['source_sha256'] and sha(binary['path'])==binary['sha256']
  for name,h in info['source_sha256'].items():assert hashlib.sha256(subprocess.check_output(['git','show',info['commit']+':'+name])).hexdigest()==h
  for name,h in m['build_sha256'].items():assert sha(name)==h and hashlib.sha256(subprocess.check_output(['git','show',info['commit']+':'+name])).hexdigest()==h
 assert meta['binaries']['baseline']['sha256']!=meta['binaries']['candidate']['sha256']
 cases=load('cases.json');assert len(cases)==len(set(cases))==20;targets=set(cases[:5]);assert targets=={f'compare/tx_rollback_1row_{n}/lin'for n in ['1k','10k','100k']}|{f'compare/durable_rollback_1row_{n}/lin'for n in ['1k','10k']}
 smokes=load('qualification-smoke-status.json');assert len(smokes)==40 and {(r['role'],r['case'])for r in smokes}=={(role,c)for role in ['baseline','candidate']for c in cases}
 for r in smokes:
  assert r['exit']==0;d=load('qualification-smoke/'+r['role']+'-'+r['case'].replace('/','_')+'/run.json');assert d['status']=='complete'and len(d['observations'])==1 and d['cases'][0]['id']==r['case']
 records=load('process-status.json');assert len(records)==720
 planned=schedule([dict(id=c)for c in cases],['baseline','control','candidate'])
 assert [(r['round'],r['mode'],r['case'])for r in records]==[(r['round'],r['role'],r['workload'])for r in planned]
 orders_from_records(records,cases,['baseline','control','candidate'],'case',role_key='mode')
 values={};ids=set();total=0
 for r in records:
  binary=meta['binaries']['baseline'if r['mode']=='control'else r['mode']];assert r['exit_code']==0 and r['binary_sha256']==binary['sha256']
  dest=p/f"{r['round']:02}-{r['mode']}"/r['case'].replace('/','_')
  assert r['command']==[binary['path'],'--filter',r['case'],'--samples','8','--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output',str(dest)]
  d=json.loads((dest/'run.json').read_text());assert d['status']=='complete'and len(d['cases'])==1 and d['cases'][0]['id']==r['case']and len(d['observations'])==8
  assert d['id']not in ids;ids.add(d['id']);assert d['provenance']['samples']=='8'and d['provenance']['warmup_ns']=='0'
  samples=[]
  for o in d['observations']:
   assert o['case']==r['case']and o['metric']=='wall'and o['operations']==1 and o['availability']['state']=='available'
   value=float(o['value']);assert math.isfinite(value)and value>0;samples.append(value);total+=1
  values[r['round'],r['mode'],r['case']]=median(samples)
 assert total==5760
 summary=load('summary.json');assert set(summary)==set(cases);accepted=True
 for c in cases:
  assert set(summary[c])=={'series1','series2'}
  for label,start in [('series1',1),('series2',7)]:
   pairs=[]
   for i in range(start,start+6):
    a,b,d=[values[i,r,c]for r in ['baseline','control','candidate']];pairs.append(dict(round=i,baseline_ns=a,control_ns=b,candidate_ns=d,gain_percent=100*(1-d/a),aa_percent=100*(1-b/a)))
   gain=median(x['gain_percent']for x in pairs);noise=median(abs(x['aa_percent'])for x in pairs);positive=sum(x['gain_percent']>0 for x in pairs);passed=(positive==6 and gain>noise)if c in targets else gain>=-max(2,noise);accepted &=passed
   assert summary[c][label]==dict(target=c in targets,gain_percent=gain,aa_absolute_percent=noise,positive_pairs=positive,passes=passed,pairs=pairs)
 q=load('qualification.json');assert q==dict(measurement_complete=True,component_gate_passes=accepted,full_engine_qualified=False,full_objective_achieved=False,processes=720,observations=5760)
 return dict(audit_complete=True,processes=720,observations=5760,component_gate_passes=accepted,synthetic_fixture=bool(env.get('synthetic_fixture')),full_objective_achieved=False)

if __name__=='__main__':
 p=HERE;result=verify(p)
 (p/'independent-audit.json').write_text(json.dumps(result,indent=2)+'\n')
 print('Independent audit PASS: 720 processes /5760 observations; component gate:',result['component_gate_passes'])
