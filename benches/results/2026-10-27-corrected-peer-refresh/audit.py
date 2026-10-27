from pathlib import Path
import json,hashlib,math,statistics as st
r=Path(__file__).resolve().parent;env=json.loads((r/'environment.json').read_text());assert env['status']=='finished';assert env['source_unchanged'];assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in env['source_sha256'].items())
assert len(env['cleanup'])==3 and all(x=={'stop_exit':0,'remove_exit':0} for x in env['cleanup'].values())
assert hashlib.sha256(Path('/tmp/lin-corrected-peer-current').read_bytes()).hexdigest()==env['binary_sha256']
out=[];counts={};versions={}
for mode in ['read','native']:
 comparisons=[];sample_count=0
 for n in [1000,10000,100000]:
  d=json.loads((r/mode/f'rows-{n}/run.json').read_text());assert d['status']=='complete' and not d['errors'];assert d['repeats']==3 and d['samples']==12;assert not d['source_drift'];assert d['lin_binary_info']['sha256']==env['binary_sha256'];versions.update(d['versions'])
  expected_engines={'lin','sqlite','duckdb','postgres','mysql','mongo'}|({'pandas'} if mode=='read' else set());assert set(d['engines'])==expected_engines
  for case in d['cases']:
   for engine in d['engines']:
    obs=[x for x in d['observations'] if x['case']==f'{case}/{engine}'];assert len(obs)==3 and {x['repetition'] for x in obs}=={0,1,2}
    for o in obs:
     assert len(o['samples_ns'])==12 and o['iterations']>0 and all(math.isfinite(x) and x>0 for x in o['samples_ns']);sample_count+=12
    value=st.median(st.median(o['samples_ns']) for o in obs);assert value==d['medians_ns'][f'{case}/{engine}']
   lin=d['medians_ns'][f'{case}/lin']
   for peer in sorted(expected_engines-{'lin'}):
    ratios=[]
    for repetition in range(3):
     vals={engine:st.median(next(x for x in d['observations'] if x['case']==f'{case}/{engine}' and x['repetition']==repetition)['samples_ns']) for engine in ['lin',peer]};ratios.append(vals[peer]/vals['lin'])
    comparisons.append({'rows':n,'case':case,'peer':peer,'lin_ns':lin,'peer_ns':d['medians_ns'][f'{case}/{peer}'],'speedup':d['medians_ns'][f'{case}/{peer}']/lin,'process_speedups':ratios,'positive_processes':sum(x>1 for x in ratios)})
  verify=json.loads((r/mode/'verification.json').read_text());v=next(x for x in verify['runs'] if x['rows']==n);assert v['sentinels_unchanged'] and all(not x for x in v['fixture_objects_remaining'].values())
 counts[mode]={'comparisons':len(comparisons),'wins':sum(x['speedup']>1 for x in comparisons),'samples':sample_count,'process_comparisons':len(comparisons)*3,'positive_processes':sum(x['positive_processes'] for x in comparisons),'minimum_speedup':min(x['speedup'] for x in comparisons)}
 out.append({'mode':mode,**counts[mode],'comparisons':comparisons})
(r/'audit.json').write_text(json.dumps({'source_commit':env['source_commit'],'versions':versions,'summary':counts,'results':out},indent=2)+'\n');print(json.dumps(counts,indent=2))
assert counts['read']['comparisons']==108 and counts['native']['comparisons']==15
