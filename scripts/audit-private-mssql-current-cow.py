#!/usr/bin/env python3
"""Validate frozen current-COW MSSQL artifacts; never print comparative outcomes."""
from pathlib import Path
import argparse,json,hashlib,statistics as st,math,subprocess,os
ROOT=Path(__file__).resolve().parents[1]
HEAD='75f8f6b7f7f03dc22a64cb963264f33e69f7da9d'
CASES=['point_get','filter_eq','text_substr','materialize','join_inner','join_filter']
def main():
 parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('private_directory',type=Path);args=parser.parse_args();private=args.private_directory.resolve()
 assert '.airbug-bench'in private.parts and 'private-mssql'in private.parts,'Comparative audit output must remain in the ignored private area'
 raw=private/'lin-mssql-private';metadata=json.loads((private/'run-metadata.json').read_text());assert metadata['status']=='completed'and metadata['head_sha']==HEAD and metadata['id']==40603880905
 env=json.loads((raw/'environment.json').read_text());assert env['status']=='finished'and env['source_unchanged']and env['source_commit']==HEAD and env['architecture']=='x86_64'
 assert env['cleanup']=={'stop_exit':0,'remove_exit':0}
 assert env['image']['Architecture']=='amd64'and env['image']['Os']=='linux'and env['image']['RepoDigests']
 for name,digest in env['source_sha256'].items():
  path=Path(name);assert not path.is_absolute()and'..'not in path.parts
  data=subprocess.check_output(['git','show',HEAD+':'+name],cwd=ROOT);assert hashlib.sha256(data).hexdigest()==digest,name
 expected={(mode,n)for mode in ['read','native']for n in [1000,10000,100000]};runs=env['runs'];assert len(runs)==6 and {(r['mode'],r['rows'])for r in runs}==expected
 for r in runs:assert r['exit_code']in [0,3]and r['sentinel_unchanged']and not r['fixture_objects_remaining']
 audit=[];sample_count=0
 for mode,n in sorted(expected):
  directory=raw/mode/f'rows-{n}';data=json.loads((directory/'run.json').read_text());cases=CASES if mode=='read'else['insert_native']
  assert data['status']=='complete'and not data['errors']and not data['source_drift']and data['git_commit']==HEAD
  assert data['rows']==n and data['samples']==12 and data['repeats']==3 and set(data['engines'])=={'lin','mssql'}and data['cases']==cases
  assert data['lin_binary_info']['sha256']==env['binary_sha256']
  assert len(data['observations'])==len(cases)*6 and {(o['case'],o['repetition'])for o in data['observations']}=={(case+'/'+engine,repeat)for case in cases for engine in ['lin','mssql']for repeat in range(3)}
  children=[]
  for repeat in range(3):
   child=json.loads((directory/f'process-{repeat+1}'/'run.json').read_text());assert child['status']=='complete'and not child['errors']and not child['source_drift']and child['git_commit']==HEAD
   assert child['repeats']==1 and child['samples']==12 and child['lin_binary_info']['sha256']==env['binary_sha256']
   assert len(child['observations'])==len(cases)*2
   children.append(child)
  for case in cases:
   expected_rows={'point_get':1,'filter_eq':1,'text_substr':1,'materialize':n//2,'join_inner':n,'join_filter':sum(i%200>100 for i in range(n)),'insert_native':n}[case]
   process_medians={}
   for engine in ['lin','mssql']:
    rows=[o for o in data['observations']if o['case']==case+'/'+engine]
    for o in rows:
     assert len(o['samples_ns'])==12 and o['iterations']>0 and o['validated_rows']==expected_rows
     assert all(type(x)in [int,float]and math.isfinite(x)and x>0 for x in o['samples_ns']);sample_count+=12
     child=children[o['repetition']];original=next(x for x in child['observations']if x['case']==o['case']);assert {**original,'repetition':o['repetition']}==o
     assert child['medians_ns'][o['case']]==st.median(o['samples_ns'])
    process_medians[engine]={o['repetition']:st.median(o['samples_ns'])for o in rows}
    assert data['medians_ns'][case+'/'+engine]==st.median(process_medians[engine].values())
   ratios=[process_medians['mssql'][i]/process_medians['lin'][i]for i in range(3)]
   audit.append(dict(mode=mode,rows=n,case=case,process_speedups=ratios,aggregate_speedup=data['medians_ns'][case+'/mssql']/data['medians_ns'][case+'/lin']))
  if mode=='native':assert set(data['native_contracts'])=={'lin','mssql'}
 assert len(audit)==21 and sample_count==1512
 result=dict(functional_raw_audit_complete=True,source_commit=HEAD,workflow_run=40603880905,source_candidate_qualified=False,full_objective_achieved=False,scope='Private read/native API comparison; not Full durability or Rust single-DML qualification',versions=env['server'],samples=sample_count,comparisons=audit)
 destination=private/'private-audit.json';assert not destination.exists(),'Preserve any existing private audit'
 descriptor=os.open(destination,os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600)
 with os.fdopen(descriptor,'w')as out:json.dump(result,out,indent=2);out.write('\n')
 print('Frozen MSSQL functional/raw audit passed; comparative outcomes remain private.')
if __name__=='__main__':main()
