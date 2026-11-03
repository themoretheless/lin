from pathlib import Path
import argparse,hashlib,json,math,subprocess,sys,platform,statistics as st
parser=argparse.ArgumentParser(description="Frozen full-operation grouped SUM discovery; all values validated outside root worker timers")
parser.add_argument('--output',type=Path,required=True)
parser.add_argument('--baseline-binary',type=Path,required=True)
parser.add_argument('--candidate-binary',type=Path,required=True)
args=parser.parse_args()
root=Path.cwd(); out=args.output.resolve(); out.mkdir(exist_ok=False)
plan=json.loads((Path(__file__).resolve().parent/'plan.json').read_text()); (out/'plan.json').write_text(json.dumps(plan,indent=2))
bins={'baseline':args.baseline_binary.resolve(),'candidate':args.candidate_binary.resolve()}; hashes={k:hashlib.sha256(v.read_bytes()).hexdigest() for k,v in bins.items()}; assert hashes['baseline']!=hashes['candidate']
meta={'platform':platform.platform(),'baseline_runtime':plan['baseline_runtime'],'candidate_commit':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'binary_sha256':hashes,'source_sha256':{str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in Path('src').glob('*.rs')},'rustc':subprocess.check_output(['rustc','-Vv'],text=True),'external_peer_qualification':False}; (out/'provenance.json').write_text(json.dumps(meta,indent=2))
cases={
 'sum_by':('orders | sum total by user_id',['user_id','total']),
 'sum_by_filtered':('orders | total > 100 | sum total by user_id',['user_id','total']),
 'sum':('orders | sum total',['total']),
 'count_by':('orders | count by user_id',['user_id','hits']),
 'point_get':('docs | id == "d-20" | {id,title}',['id','title']),
 'join_filter':('orders | total > 100 | join users on user_id | {id,users.email,total} | take all',['id','users.email','total'])}
records=[]
for round in range(1,13):
 order=plan['roles'][(round-1)%3:]+plan['roles'][:(round-1)%3]
 for n in plan['sizes']:
  users=max(1,n//10); sums={}; filtered={}; counts={}
  for i in range(n):
   k=f'u-{i%users}'; sums[k]=sums.get(k,0.0)+i%200; counts[k]=counts.get(k,0)+1
   if i%200>100:filtered[k]=filtered.get(k,0.0)+i%200
  want={'sum_by':list(map(list,sums.items())),'sum_by_filtered':list(map(list,filtered.items())),'count_by':list(map(list,counts.items())),'sum':[[sum(i%200 for i in range(n))]],'point_get':[['d-20','doc 20 wal note']],'join_filter':[[f'o-{i}',f'u{i%users}@bench.local',i%200] for i in range(n) if i%200>100]}
  for role in order:
   executable=bins['baseline' if role=='control' else role]
   assert hashlib.sha256(executable.read_bytes()).hexdigest()==hashes['baseline' if role=='control' else role]
   p=subprocess.Popen([str(executable)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
   def ask(request):
    p.stdin.write(json.dumps(request)+'\n'); p.stdin.flush(); line=p.stdout.readline(); assert line,'worker exited'; return json.loads(line)
   try:
    ready=ask({'rows':n}); assert ready['ready']
    for case,(source,fields) in cases.items():
     request={'source':source,'fields':fields,'iterations':plan['iterations'],'samples':plan['samples']}; d=ask(request)
     assert sorted(map(tuple,d['result']))==sorted(map(tuple,want[case])),case
     assert len(d['samples_ns'])==24 and all(type(v) in [int,float] and math.isfinite(v) and v>0 for v in d['samples_ns'])
     name=f'{round:02}-{n}-{role}-{case}.json'; (out/name).write_text(json.dumps({'request':request,'response':d})); records.append({'round':round,'rows':n,'role':role,'case':case,'file':name,'median_ns':st.median(d['samples_ns']),'result_rows':len(d['result'])})
   finally:
    p.stdin.close(); status=p.wait(); assert status==0,p.stderr.read(); p.stdout.close(); p.stderr.close()
   (out/'records.json').write_text(json.dumps(records,indent=2)); print(round,n,role,'complete',flush=True)
assert len(records)==648
summary=[]
for n in plan['sizes']:
 for case in cases:
  for series,start in [(1,1),(2,7)]:
   values={(r['round'],r['role']):r['median_ns'] for r in records if r['rows']==n and r['case']==case}
   pairs=[{'gain':100*(1-values[i,'candidate']/values[i,'baseline']),'aa':abs(100*(1-values[i,'control']/values[i,'baseline']))} for i in range(start,start+6)]
   gain=st.median(p['gain'] for p in pairs); aa=st.median(p['aa'] for p in pairs); positive=sum(p['gain']>0 for p in pairs); target=case in plan['targets']; ok=positive==6 and gain>aa if target else gain>=-max(2,aa)
   summary.append({'rows':n,'case':case,'series':series,'target':target,'gain_percent':gain,'aa_absolute_percent':aa,'positive_pairs':positive,'passes':ok,'pairs':pairs})
assert all(hashlib.sha256(Path(p).read_bytes()).hexdigest()==h for p,h in meta['source_sha256'].items())
(out/'summary.json').write_text(json.dumps(summary,indent=2)); (out/'decision.json').write_text(json.dumps({'complete':True,'processes':108,'case_observations':648,'wall_samples':15552,'component_gate_passes':all(r['passes'] for r in summary),'full_source_qualified':False,'full_objective_achieved':False},indent=2)); print('FAILED WINDOWS',json.dumps([r for r in summary if not r['passes']]),flush=True)
