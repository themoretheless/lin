from pathlib import Path
import hashlib,json,math,statistics as st
from source_manifest import manifest
p=Path('/tmp/lin-current-candidate-public-peers-20261103');m=json.loads((p/'environment.json').read_text());assert manifest()==m['source_sha256'];assert m['status']=='finished'and m['source_unchanged'];sha=m['binary_sha256'];assert hashlib.sha256(Path('/tmp/lin-current-candidate-public-peer-built-20261103').read_bytes()).hexdigest()==sha
assert json.loads((p/'cleanup-proof.json').read_text())['all_owned_containers_absent'];assert len(m['images'])==3 and all(i['Architecture']=='amd64'and i['Os']=='linux'for i in m['images'].values());processes=samples=0
for mode in ['read','native']:
 cases=['point_get','filter_eq','text_substr','materialize','join_inner','join_filter']if mode=='read'else['insert_native'];engines={'lin','sqlite','duckdb','postgres','mysql','mongo'}|({'pandas'}if mode=='read'else set())
 for n in [1000,10000,100000]:
  folder=p/mode/f'rows-{n}';d=json.loads((folder/'run.json').read_text());assert d['status']=='complete'and not d['errors']and not d['source_drift']and d['git_commit']==m['source_commit']and d['lin_binary_info']['sha256']==sha
  assert d['rows']==n and d['samples']==12 and d['repeats']==3 and d['cases']==cases and set(d['engines'])==engines
  coords={(o['case'],o['repetition'])for o in d['observations']};assert len(d['observations'])==len(coords)and coords=={(c+'/'+e,i)for c in cases for e in engines for i in range(3)}
  for i in range(3):
   child=json.loads((folder/f'process-{i+1}'/'run.json').read_text());processes+=1;assert child['status']=='complete'and not child['errors']and not child['source_drift']and child['git_commit']==m['source_commit']and child['lin_binary_info']['sha256']==sha
   assert set(child['engines'])==engines and child['cases']==cases and len(child['observations'])==len(cases)*len(engines)
   for o in child['observations']:
    assert o['repetition']==0 and o['iterations']>0 and len(o['samples_ns'])==12 and all(math.isfinite(v)and v>0 for v in o['samples_ns'])
    parent=next(x for x in d['observations']if x['case']==o['case']and x['repetition']==i);assert o['samples_ns']==parent['samples_ns'];samples+=12
assert (processes,samples)==(18,5184)
(p/'process-audit.json').write_text(json.dumps({'processes_verified':processes,'wall_samples':samples,'runtime_source_unchanged':True,'full_objective_achieved':False},indent=2)+'\n');print('All 18 original processes and 5184 samples verified')
