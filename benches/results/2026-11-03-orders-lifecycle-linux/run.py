"""Full seed/query lifecycle stability gate; not isolated cold-query timing."""
from pathlib import Path
import argparse,hashlib,itertools,json,math,os,statistics as st,subprocess,time
p=argparse.ArgumentParser();p.add_argument('--output',type=Path,required=True);p.add_argument('--baseline-binary',type=Path,required=True);p.add_argument('--candidate-binary',type=Path,required=True);a=p.parse_args()
out=a.output;out.mkdir(exist_ok=False)
bins={'baseline':a.baseline_binary.resolve(),'candidate':a.candidate_binary.resolve()}
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
hashes={k:sha(v) for k,v in bins.items()};assert hashes['baseline']!=hashes['candidate']
records=[];orders=list(itertools.permutations(['baseline','control','candidate']))
for r in range(1,13):
 for n in [1000,10000,100000]:
  users=max(1,n//10)
  requests=[{'rows':n},{'source':'orders | sum total','fields':['total'],'samples':1,'iterations':1},{'source':'orders | {id,user_id,total} | take all','fields':['id','user_id','total'],'samples':1,'iterations':1}]
  expected=[[f'o-{i}',f'u-{i%users}',i%200] for i in range(n)]
  for role in orders[(r-1)%6]:
   binary=bins['baseline' if role=='control' else role];assert sha(binary)==hashes['baseline' if role=='control' else role]
   stem=f'{r:02}-{n}-{role}';stdin=out/(stem+'.stdin');stdout=out/(stem+'.json');stderr=out/(stem+'.stderr')
   stdin.write_text(''.join(json.dumps(q)+'\n' for q in requests))
   with stdin.open() as si,stdout.open('w') as so,stderr.open('w') as se:
    start=time.perf_counter_ns();child=subprocess.Popen([str(binary)],stdin=si,stdout=so,stderr=se)
    pid,status,usage=os.wait4(child.pid,0);elapsed=time.perf_counter_ns()-start;child.returncode=os.waitstatus_to_exitcode(status)
   assert pid==child.pid and child.returncode==0 and not stderr.read_text()
   data=[json.loads(line) for line in stdout.read_text().splitlines()]
   assert len(data)==3 and data[0]['ready'] is True
   assert data[1]['result']==[[sum(i%200 for i in range(n))]]
   assert data[2]['result']==expected
   for response in data[1:]:
    assert len(response['samples_ns'])==1 and math.isfinite(response['samples_ns'][0]) and response['samples_ns'][0]>0
   assert elapsed>0 and usage.ru_maxrss>0
   records.append(dict(round=r,rows=n,role=role,file=stdout.name,binary_sha256=sha(binary),process_wall_ns=elapsed,peak_rss_kib=usage.ru_maxrss,process_user_cpu_s=usage.ru_utime,process_system_cpu_s=usage.ru_stime))
   (out/'records.json').write_text(json.dumps(records,indent=2)+'\n');print(stem,'seed, SUM and complete orders readback verified',flush=True)
assert len(records)==108
windows=[]
for n in [1000,10000,100000]:
 for metric,floor in [('process_wall_ns',2),('peak_rss_kib',5)]:
  values={(v['round'],v['role']):v[metric] for v in records if v['rows']==n}
  for series,start in [(1,1),(2,7)]:
   gains=[100*(1-values[r,'candidate']/values[r,'baseline']) for r in range(start,start+6)]
   aa=[abs(100*(1-values[r,'control']/values[r,'baseline'])) for r in range(start,start+6)]
   gain,noise=st.median(gains),st.median(aa)
   windows.append(dict(rows=n,metric=metric,series=series,gains_percent=gains,aa_absolute_percent=aa,median_gain_percent=gain,median_aa_percent=noise,tolerance_percent=max(floor,noise),passes=gain>=-max(floor,noise)))
(out/'summary.json').write_text(json.dumps(windows,indent=2)+'\n')
(out/'decision.json').write_text(json.dumps(dict(complete=True,processes=108,lifecycle_resource_gate_passes=all(v['passes'] for v in windows),isolated_first_query_qualified=False,full_peer_qualified=False,full_objective_achieved=False),indent=2)+'\n')
print('Failed lifecycle windows:',json.dumps([v for v in windows if not v['passes']]),flush=True)
