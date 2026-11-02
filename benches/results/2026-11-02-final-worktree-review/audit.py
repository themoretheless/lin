"""Recompute preserved measurements; does not launch benchmarks or change gates."""
import argparse, hashlib, json, math, statistics as st, subprocess, tarfile
from pathlib import Path
out=Path(__file__).resolve().parent
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--cow-root',type=Path,required=True)
parser.add_argument('--batch-root',type=Path,required=True)
parser.add_argument('--update-root',type=Path,required=True)
args=parser.parse_args()
cow,batch,update=args.cow_root,args.batch_root,args.update_root
load=lambda p:json.loads(p.read_text())
sha=lambda b:hashlib.sha256(b).hexdigest()
result={}
# Every archived COW member is independently read and checked, without extraction.
for name in ['2026-11-02-cow-extent-full-engine-linux','2026-11-02-cow-extent-public-peers-linux']:
 p=cow/'benches/results'/name; manifest=load(p/'raw-manifest.json'); seen=set()
 with tarfile.open(p/'raw.tar.gz','r:gz') as tf:
  for member in tf:
   if not member.isfile(): continue
   assert member.name not in seen
   data=tf.extractfile(member).read(); expected=manifest[member.name]
   assert sha(data)==expected['sha256']
   assert len(data)==expected.get('bytes',expected.get('size'))
   seen.add(member.name)
 assert seen==set(manifest)
 result[name]={'members_verified':len(seen),'decision':load(p/'decision.json') if (p/'decision.json').exists() else (p/'decision.md').read_text()}
assert not subprocess.check_output(['git','-C',str(cow),'diff','60e64559e','HEAD','--','src','Cargo.toml','Cargo.lock'])
# Independently reconstruct the complete UPDATE matrix and original gate.
provenance=load(update/'provenance.json'); commit=provenance['source_commit']
for name,digest in provenance['source_sha256'].items(): assert sha(subprocess.check_output(['git','show',commit+':'+name]))==digest
assert sha(subprocess.check_output(['git','show',commit+':benches/results/2026-11-01-update-patch-clone-elision/compare.rs']))==provenance['compare_sha256']
for name,digest in load(update/'frozen-inputs.json').items(): assert sha(subprocess.check_output(['git','show',commit+':'+name]))==digest
spec=load(update/'input.json')
for role,info in provenance['binaries'].items():
 for name,digest in info['role_source_sha256'].items():
  ref=spec['baseline_reference'] if role=='baseline' else commit
  assert sha(subprocess.check_output(['git','show',ref+':'+name]))==digest
smokes=load(update/'qualification-smoke-status.json'); assert len(smokes)==28 and all(s['exit']==0 for s in smokes)
assert load(update/'test-status.json')['release_passed']==455
mut=load(update/'mutant-status.json'); assert mut['source_restored'] and len(mut['checks'])==2 and all(c['compiled_runtime_rejected'] for c in mut['checks'])
records=load(update/'process-status.json'); assert len(records)==504
summary=load(update/'summary.json'); med={}; observations=0; coords=set()
for r in records:
 coordinate=(r['round'],r['mode'],r['case']); assert coordinate not in coords; coords.add(coordinate)
 assert r['exit_code']==0 and r['binary_sha256']==provenance['binaries']['baseline' if r['mode']=='control' else r['mode']]['sha256']
 p=update/f"{r['round']:02}-{r['mode']}"/r['case'].replace('/','_')/'run.json'; d=load(p)
 assert d['status']=='complete' and len(d['observations'])==8 and len(d['cases'])==1 and d['cases'][0]['id']==r['case']
 values=[]
 for v in d['observations']:
  assert v['case']==r['case'] and v['metric']=='wall' and v['operations']==1 and v['availability']['state']=='available'
  value=float(v['value']); assert math.isfinite(value) and value>0; values.append(value); observations+=1
 med[coordinate]=st.median(values)
assert coords=={(i,m,c) for i in range(1,13) for m in ['baseline','control','candidate'] for c in summary}
failed=[]; passed_targets=0
for case,series_rows in summary.items():
 for series,expected in series_rows.items():
  start=1 if series=='series1' else 7
  gains=[100*(1-med[i,'candidate',case]/med[i,'baseline',case]) for i in range(start,start+6)]
  aa=[abs(100*(1-med[i,'control',case]/med[i,'baseline',case])) for i in range(start,start+6)]
  gain=st.median(gains); noise=st.median(aa); positive=sum(g>0 for g in gains)
  ok=(positive==6 and gain>noise) if expected['target'] else gain>=-max(2,noise)
  assert math.isclose(gain,expected['gain_percent'],abs_tol=1e-10) and math.isclose(noise,expected['aa_absolute_percent'],abs_tol=1e-10)
  assert positive==expected['positive_pairs'] and ok==expected['passes']
  passed_targets+=bool(expected['target'] and ok)
  if not ok: failed.append({'case':case,'series':series,'gain_percent':gain,'aa_absolute_percent':noise,'positive_pairs':positive,'target':expected['target']})
assert observations==4032 and failed and not load(update/'qualification.json')['component_gate_passes']
result['update_clone_elision']={'run':41251433614,'processes':504,'observations':observations,'passed_target_windows':passed_targets,'failed_windows':failed,'source_verified':True}
# Recompute both historical shape-template series from all raw observations.
p=batch/'benches/results/2026-10-24-batch-row-shape-template'
assert (batch/'src/exec.rs').read_bytes()==(p/'candidate-exec.rs').read_bytes()
assert (p/'before-exec.rs').read_bytes()==subprocess.check_output(['git','show','518db365d:src/exec.rs'])
series_results=[]
for folder in [p,p/'repeat']:
 expected=load(folder/'medians.json'); recomputed={}
 for case,table in expected.items():
  mode='durable' if 'durable_insert' in case else 'native'; values={}
  for variant in ['baseline','candidate']:
   values[variant]=[]
   for i in range(1,7):
    d=load(folder/f'{mode}-{variant}-{i}'/'run.json'); assert d['status']=='complete'
    samples=[float(x['value'])/x['operations'] for x in d['observations'] if x['case']==case]
    assert len(samples)==24 and all(math.isfinite(v) and v>0 for v in samples)
    values[variant].append(st.median(samples))
   assert values[variant]==table[variant+'_ns']
  gain=st.median(100*(a-b)/a for a,b in zip(values['baseline'],values['candidate'])); wins=sum(b<a for a,b in zip(values['baseline'],values['candidate']))
  assert math.isclose(gain,table['paired_gain_percent'],abs_tol=1e-10) and wins==table['wins']
  recomputed[case]={'gain_percent':gain,'positive_pairs':wins}
 series_results.append(recomputed)
assert load(p/'workspace-status.json')['exit_code']==0
result['batch_shape']={'processes':48,'observations':4608,'source_matches_measured_candidate':True,'series':series_results,'historical_workspace_exit':0,'decision':'retain: repeated native gain, durable 1k inconclusive; no current-main qualification'}
(out/'audit.json').write_text(json.dumps(result,indent=2)+'\n')
print('COW archives verified; UPDATE 504/4032 independently recomputed; batch-shape 48 processes/4608 observations independently recomputed.')
print('UPDATE passed target windows:',passed_targets,'failed windows:',len(failed))
