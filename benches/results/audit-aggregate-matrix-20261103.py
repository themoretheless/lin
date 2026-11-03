"""Independently rebuild aggregate source-matrix values and frozen numeric windows."""
import argparse,hashlib,json,math,statistics as st,subprocess,tarfile
from pathlib import Path
parser=argparse.ArgumentParser();parser.add_argument('--raw',type=Path,required=True);parser.add_argument('--output',type=Path,required=True);args=parser.parse_args();p=args.raw;out=args.output;out.mkdir(exist_ok=True)
load=lambda f:json.loads(f.read_text());meta=load(p/'provenance.json');records=load(p/'records.json');plan=load(p/'plan.json');summary=load(p/'summary.json');roles=plan['roles'];cases=plan['targets']+plan['controls'];assert len(cases)==len(set(cases));expected={(i,n,role,c) for i in range(1,13) for n in plan['sizes'] for role in roles for c in cases};assert {(r['round'],r['rows'],r['role'],r['case']) for r in records}==expected and len(records)==len(expected)
for name,digest in meta['source_sha256'].items():assert hashlib.sha256(subprocess.check_output(['git','show',meta['candidate_commit']+':'+name])).hexdigest()==digest
queries={'sum_by':('orders | sum total by user_id',['user_id','total']),'sum_by_filtered':('orders | total > 100 | sum total by user_id',['user_id','total']),'sum':('orders | sum total',['total']),'count_by':('orders | count by user_id',['user_id','hits']),'point_get':('docs | id == "d-20" | {id,title}',['id','title']),'join_filter':('orders | total > 100 | join users on user_id | {id,users.email,total} | take all',['id','users.email','total'])}
med={};samples=0
for r in records:
 d=load(p/r['file']);n=r['rows'];users=max(1,n//10);case=r['case'];want=[];request=d['request'];response=d['response']
 if case=='insert_native':
  want=[[f'd-{i}',f'bench://{i}','rag' if i%2==0 else 'sys',f'doc {i} wal note' if i%10==0 else f'doc {i} plain',123456,f'body {i}'] for i in range(n)]
  assert request=={'insert_rows':n,'samples':plan['native_control_samples'],'timestamp_ms':123456}
  assert response['validated_result_rows']==n and response['result_sha256']==hashlib.sha256(json.dumps(want,separators=(',',':')).encode()).hexdigest()
  assert response['statements']==(n+9999)//10000 and response['statement_rows_limit']==10000
 else:
  source,fields=queries[case];assert request=={'source':source,'fields':fields,'iterations':plan.get('point_get_iterations',plan['iterations']) if case=='point_get' else plan['iterations'],'samples':plan['samples']}
  if case in ['sum_by','sum_by_filtered','count_by']:
   groups={}
   for i in range(n):
    if case=='sum_by_filtered' and i%200<=100:continue
    key=f'u-{i%users}';groups[key]=groups.get(key,0)+(1 if case=='count_by' else i%200)
   want=list(map(list,groups.items()))
  elif case=='sum':want=[[sum(i%200 for i in range(n))]]
  elif case=='point_get':want=[['d-20','doc 20 wal note']]
  else:want=[[f'o-{i}',f'u{i%users}@bench.local',i%200] for i in range(n) if i%200>100]
  assert sorted(map(tuple,response['result']))==sorted(map(tuple,want))
 values=response['samples_ns'];assert len(values)==(plan['native_control_samples'] if case=='insert_native' else plan['samples']);assert all(math.isfinite(v) and v>0 for v in values);samples+=len(values);median=st.median(values);assert median==r['median_ns'];med[n,case,r['round'],r['role']]=median
for row in summary:
 n,c=row['rows'],row['case'];start=1 if row['series']==1 else 7
 gains=[100*(1-med[n,c,i,'candidate']/med[n,c,i,'baseline']) for i in range(start,start+6)];aa=[abs(100*(1-med[n,c,i,'control']/med[n,c,i,'baseline'])) for i in range(start,start+6)];gain,noise=st.median(gains),st.median(aa);positive=sum(g>0 for g in gains);target=c in plan['targets'];ok=positive==6 and gain>noise if target else gain>=-max(2,noise)
 assert row['target']==target and row['gain_percent']==gain and row['aa_absolute_percent']==noise and row['positive_pairs']==positive and row['passes']==ok
 if 'production' in roles:
  assert row['production_gain_percent']==st.median(100*(1-med[n,c,i,'candidate']/med[n,c,i,'production']) for i in range(start,start+6));assert row['production_positive_pairs']==sum(med[n,c,i,'candidate']<med[n,c,i,'production'] for i in range(start,start+6))
assert len(summary)==len(plan['sizes'])*len(cases)*2
result={'complete':True,'processes':12*len(plan['sizes'])*len(roles),'case_observations':len(records),'wall_samples':samples,'passed_target_windows':sum(r['target'] and r['passes'] for r in summary),'total_target_windows':sum(r['target'] for r in summary),'failed_control_windows':sum(not r['target'] and not r['passes'] for r in summary),'component_gate_passes':all(r['passes'] for r in summary),'full_objective_achieved':False};assert result['component_gate_passes']==load(p/'decision.json')['component_gate_passes'];(out/'audit.json').write_text(json.dumps(result,indent=2)+'\n')
for name in ['summary.json','decision.json','provenance.json']:(out/name).write_bytes((p/name).read_bytes())
manifest={}
with tarfile.open(out/'raw.tar.gz','w:gz') as tf:
 for f in sorted(p.glob('*.json')):
  b=f.read_bytes();manifest[f.name]={'bytes':len(b),'sha256':hashlib.sha256(b).hexdigest()};tf.add(f,arcname=f.name,recursive=False)
with tarfile.open(out/'raw.tar.gz','r:gz') as tf:
 for m in tf:
  b=tf.extractfile(m).read();assert manifest[m.name]=={'bytes':len(b),'sha256':hashlib.sha256(b).hexdigest()}
(out/'raw-manifest.json').write_text(json.dumps(manifest,indent=2)+'\n');print(json.dumps(result))
