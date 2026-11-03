"""Audit the frozen local current-main discovery and independent Linux CI artifact."""
from pathlib import Path
import argparse,hashlib,json,math,statistics as st,subprocess,tarfile
parser=argparse.ArgumentParser(); parser.add_argument('--local',type=Path,required=True);parser.add_argument('--ci',type=Path,required=True);args=parser.parse_args();out=Path(__file__).resolve().parent
load=lambda p:json.loads(p.read_text()); manifest=load(args.local/'source-manifest.json'); sha=lambda b:hashlib.sha256(b).hexdigest(); runtime=manifest['runtime_commit']; assert runtime=='0dab1a9a4f0f4f1e736d2e54f2e34a1e0f133d2d'
for name,digest in manifest['files'].items():assert sha(subprocess.check_output(['git','show',runtime+':'+name]))==digest
result={'runtime':runtime,'local':{},'linux_ci':{}}; binary_sha=None
for mode in ['read','native']:
 comparisons=[]; losses=[]; observations=0
 for n in [1000,10000,100000]:
  p=args.local/f'{mode}-{n}'; d=load(p/'run.json'); cases=['point_get','filter_eq','text_substr','materialize','join_inner','join_filter'] if mode=='read' else ['insert_native']; engines={'lin','sqlite','duckdb','pandas'} if mode=='read' else {'lin','sqlite','duckdb'}; samples=8 if mode=='read' else 48
  assert d['status']=='complete' and not d['errors'] and not d['source_drift'] and d['git_commit']==runtime
  assert d['cases']==cases and set(d['engines'])==engines and d['samples']==samples and d['repeats']==3
  binary_sha=binary_sha or d['lin_binary_info']['sha256']; assert binary_sha==d['lin_binary_info']['sha256']
  med={}; coords=set()
  for o in d['observations']:
   coord=(o['case'],o['repetition']); assert coord not in coords; coords.add(coord); assert len(o['samples_ns'])==samples and o['iterations']>0 and all(math.isfinite(x) and x>0 for x in o['samples_ns']);med[coord]=st.median(o['samples_ns']); observations+=samples
  assert coords=={(c+'/'+e,i) for c in cases for e in engines for i in range(3)}
  for i in range(3):
   child=load(p/f'process-{i+1}'/'run.json'); assert child['status']=='complete' and not child['errors'] and not child['source_drift']; assert child['lin_binary_info']['sha256']==binary_sha
   for o in child['observations']:assert med[o['case'],i]==st.median(o['samples_ns'])
  for c in cases:
   values={e:st.median(med[c+'/'+e,i] for i in range(3)) for e in engines}
   for e in engines:assert values[e]==d['medians_ns'][c+'/'+e]
   for peer in sorted(engines-{'lin'}):
    ratios=[med[c+'/'+peer,i]/med[c+'/lin',i] for i in range(3)]; row={'rows':n,'case':c,'peer':peer,'aggregate_peer_over_lin':values[peer]/values['lin'],'process_ratios':ratios}; comparisons.append(row)
    for i,ratio in enumerate(ratios):
     if ratio<=1:losses.append({'rows':n,'case':c,'peer':peer,'process':i+1,'peer_over_lin':ratio,'lin_ns':med[c+'/lin',i],'peer_ns':med[c+'/'+peer,i]})
 result['local'][mode]={'comparisons':len(comparisons),'aggregate_wins':sum(r['aggregate_peer_over_lin']>1 for r in comparisons),'process_comparisons':3*len(comparisons),'process_wins':sum(sum(v>1 for v in r['process_ratios']) for r in comparisons),'wall_samples':observations,'minimum_aggregate_peer_over_lin':min(r['aggregate_peer_over_lin'] for r in comparisons),'losses':losses,'details':comparisons}
result['local']['binary_sha256']=binary_sha
for label in ['ci','ci-mutations']:
 d=load(args.ci/'.airbug-bench'/label/'run.json'); assert d['status']=='complete'; med={}
 for o in d['observations']:
  if o['metric']!='wall':continue
  assert o['operations']>0 and math.isfinite(float(o['value'])) and float(o['value'])>0
  med.setdefault(o['case'],[]).append(float(o['value'])/o['operations'])
 losses=[]; count=0
 for key,values in med.items():
  if key.endswith('/lin'):
   for peer in ['sqlite','sqlite_wal','duckdb']:
    name=key[:-3]+peer
    if name in med:
     ratio=st.median(med[name])/st.median(values); count+=1
     if ratio<=1:losses.append({'case':key.rsplit('/',1)[0],'peer':peer,'peer_over_lin':ratio})
 result['linux_ci'][label]={'comparisons':count,'losses':losses,'single_process':True,'full_objective_achieved':False}
result['linux_ci']['run']=41522176420
(out/'audit.json').write_text(json.dumps(result,indent=2)+'\n')
for label,path in [('local',args.local),('linux-ci',args.ci)]:
 members={}
 with tarfile.open(out/(label+'-raw.tar.gz'),'w:gz') as tf:
  for f in sorted(path.rglob('*')):
   if f.is_file():
    name=str(f.relative_to(path));b=f.read_bytes();members[name]={'bytes':len(b),'sha256':sha(b)};tf.add(f,arcname=name,recursive=False)
 with tarfile.open(out/(label+'-raw.tar.gz'),'r:gz') as tf:
  for m in tf:
   b=tf.extractfile(m).read();assert members[m.name]=={'bytes':len(b),'sha256':sha(b)}
 (out/(label+'-raw-manifest.json')).write_text(json.dumps(members,indent=2)+'\n')
print({mode:{k:v for k,v in d.items() if k!='details'} for mode,d in result['local'].items() if isinstance(d,dict)})
