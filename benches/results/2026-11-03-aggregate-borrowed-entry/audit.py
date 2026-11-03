"""Audit every preserved observation; the measured orders queries bypass this prototype."""
import argparse,hashlib,json,math,statistics as st,subprocess,tarfile
from pathlib import Path
parser=argparse.ArgumentParser();parser.add_argument('--raw',type=Path,required=True);args=parser.parse_args();p=args.raw;out=Path(__file__).resolve().parent
load=lambda f:json.loads(f.read_text()); meta=load(p/'provenance.json'); records=load(p/'records.json'); plan=load(p/'plan.json'); summary=load(p/'summary.json')
assert len(records)==648 and len({(r['round'],r['rows'],r['role'],r['case']) for r in records})==648
for name,digest in meta['source_sha256'].items():assert hashlib.sha256(subprocess.check_output(['git','show',meta['candidate_commit']+':'+name])).hexdigest()==digest
before=subprocess.check_output(['git','show',plan['baseline_runtime']+':src/exec.rs'],text=True);after=subprocess.check_output(['git','show',meta['candidate_commit']+':src/exec.rs'],text=True)
def fast_path(s):return s[s.index('// 1. SoA paths for orders'):s.index('// 2. SoA paths for docs')]
assert fast_path(before)==fast_path(after),'coverage finding depends on the same orders fast path'
for role,filename in [('baseline','/tmp/lin-current-main-peer-baseline-20261103'),('candidate','/tmp/lin-current-main-peer-candidate-20261103')]:assert hashlib.sha256(Path(filename).read_bytes()).hexdigest()==meta['binary_sha256'][role]
med={};samples=0
for r in records:
 d=load(p/r['file']); assert d['request']['iterations']==4 and d['request']['samples']==24
 assert d['request']['source'].startswith(('orders |','docs |'))
 values=d['response']['samples_ns']; assert len(values)==24 and all(math.isfinite(v) and v>0 for v in values); samples+=24
 median=st.median(values);assert median==r['median_ns']; med[r['rows'],r['case'],r['round'],r['role']]=median
 n=r['rows'];users=max(1,n//10); expected={};case=r['case']
 if case in ['sum_by','sum_by_filtered','count_by']:
  for i in range(n):
   if case=='sum_by_filtered' and i%200<=100:continue
   k=f'u-{i%users}';expected[k]=expected.get(k,0)+(1 if case=='count_by' else i%200)
  want=list(map(list,expected.items()))
 elif case=='sum':want=[[sum(i%200 for i in range(n))]]
 elif case=='point_get':want=[['d-20','doc 20 wal note']]
 else:want=[[f'o-{i}',f'u{i%users}@bench.local',i%200] for i in range(n) if i%200>100]
 assert sorted(map(tuple,d['response']['result']))==sorted(map(tuple,want))
for row in summary:
 n,case=row['rows'],row['case'];start=1 if row['series']==1 else 7
 gains=[100*(1-med[n,case,i,'candidate']/med[n,case,i,'baseline']) for i in range(start,start+6)]
 aa=[abs(100*(1-med[n,case,i,'control']/med[n,case,i,'baseline'])) for i in range(start,start+6)]
 gain,noise=st.median(gains),st.median(aa);positive=sum(g>0 for g in gains);ok=positive==6 and gain>noise if row['target'] else gain>=-max(2,noise)
 assert gain==row['gain_percent'] and noise==row['aa_absolute_percent'] and positive==row['positive_pairs'] and ok==row['passes']
assert samples==15552
result={'complete':True,'processes':108,'case_observations':648,'wall_samples':samples,'target_path_exercised':False,'classification':'misdirected performance experiment: orders SoA bypasses the modified generic path','source_restored_before_SoA_repair':True,'full_objective_achieved':False}
(out/'audit.json').write_text(json.dumps(result,indent=2)+'\n')
for name in ['summary.json','decision.json','provenance.json']:(out/name).write_bytes((p/name).read_bytes())
manifest={}
with tarfile.open(out/'raw.tar.gz','w:gz') as tf:
 for f in sorted(p.glob('*.json')):
  b=f.read_bytes();manifest[f.name]={'bytes':len(b),'sha256':hashlib.sha256(b).hexdigest()};tf.add(f,arcname=f.name,recursive=False)
with tarfile.open(out/'raw.tar.gz','r:gz') as tf:
 for m in tf:
  b=tf.extractfile(m).read();assert manifest[m.name]=={'bytes':len(b),'sha256':hashlib.sha256(b).hexdigest()}
(out/'raw-manifest.json').write_text(json.dumps(manifest,indent=2)+'\n');print(json.dumps(result))
