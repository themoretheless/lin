"""Frozen isolated orders mutation and aggregate-summary gate."""
from pathlib import Path
import argparse,hashlib,itertools,json,math,os,statistics as st,subprocess,time
PHASES=['insert','first_sum','warm_sum','update','after_update_sum','delete','after_delete_sum']
def validate(data,n):
 assert data['rows']==n and data['statement_rows_limit']==500 and len(data['samples'])==1
 sample=data['samples'][0];assert set(sample['phase_ns'])==set(PHASES)
 assert all(type(v) in [int,float] and math.isfinite(v) and v>0 for v in sample['phase_ns'].values())
 total=sum(i%200 for i in range(n));assert sample['sums']==[total,total+201,total+201-(n-1)%200]
 expected=[]
 for updated,deleted in [(False,False),(True,False),(True,True)]:
  expected.append([[f'o-{i}','u',201 if updated and i==0 else i%200] for i in range(n-int(deleted))])
 assert sample['states']==expected
 return sample['phase_ns']
parser=argparse.ArgumentParser();parser.add_argument('--output',type=Path,required=True);parser.add_argument('--baseline-binary',type=Path,required=True);parser.add_argument('--candidate-binary',type=Path,required=True);args=parser.parse_args()
out=args.output;out.mkdir(exist_ok=False)
bins={'baseline':args.baseline_binary.resolve(),'candidate':args.candidate_binary.resolve()}
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
hashes={k:sha(v) for k,v in bins.items()};assert hashes['baseline']!=hashes['candidate']
records=[];orderings=list(itertools.permutations(['baseline','control','candidate']))
for r in range(1,13):
 for n in [1000,10000,100000]:
  for role in orderings[(r-1)%6]:
   binrole='baseline' if role=='control' else role;binary=bins[binrole];assert sha(binary)==hashes[binrole]
   stem=f'{r:02}-{n}-{role}';output=out/(stem+'.json');error=out/(stem+'.stderr')
   with output.open('w') as so,error.open('w') as se:
    start=time.perf_counter_ns();child=subprocess.Popen([str(binary),str(n),'1'],stdout=so,stderr=se)
    pid,status,usage=os.wait4(child.pid,0);elapsed=time.perf_counter_ns()-start;child.returncode=os.waitstatus_to_exitcode(status)
   assert pid==child.pid and child.returncode==0 and not error.read_text()
   phases=validate(json.loads(output.read_text()),n)
   assert elapsed>0 and usage.ru_maxrss>0
   records.append(dict(round=r,rows=n,role=role,file=output.name,binary_sha256=hashes[binrole],phase_ns=phases,process_wall_ns=elapsed,peak_rss_kib=usage.ru_maxrss))
   (out/'records.json').write_text(json.dumps(records,indent=2)+'\n');print(stem,'all seven phases and three complete states verified',flush=True)
assert len(records)==108
windows=[]
for n in [1000,10000,100000]:
 for metric in PHASES+['process_wall_ns','peak_rss_kib']:
  values={(rec['round'],rec['role']):(rec['phase_ns'][metric] if metric in PHASES else rec[metric]) for rec in records if rec['rows']==n}
  for series,start in [(1,1),(2,7)]:
   gains=[100*(1-values[r,'candidate']/values[r,'baseline']) for r in range(start,start+6)]
   aa=[abs(100*(1-values[r,'control']/values[r,'baseline'])) for r in range(start,start+6)]
   gain,noise=st.median(gains),st.median(aa);positive=sum(v>0 for v in gains);target=metric=='warm_sum';tolerance=max(5 if metric=='peak_rss_kib' else 2,noise)
   passed=(positive==6 and gain>noise) if target else gain>=-tolerance
   windows.append(dict(rows=n,metric=metric,series=series,target=target,gains_percent=gains,aa_absolute_percent=aa,median_gain_percent=gain,median_aa_percent=noise,positive_pairs=positive,tolerance_percent=tolerance,passes=passed))
(out/'summary.json').write_text(json.dumps(windows,indent=2)+'\n')
(out/'decision.json').write_text(json.dumps(dict(complete=True,processes=108,phase_samples=756,source_mutation_gate_passes=all(v['passes'] for v in windows),full_peer_qualified=False,full_objective_achieved=False),indent=2)+'\n')
print('Failed windows:',json.dumps([v for v in windows if not v['passes']]),flush=True)
