from pathlib import Path
import tempfile,json,subprocess,shutil,math
p=Path(__file__).resolve().parent
cases=[f'compare/durable_{op}_1row_{size}/lin'for size in ['1k','10k']for op in ['update','rollback']]
roles=['original','control','disabled','traced'];checks=[]
with tempfile.TemporaryDirectory(prefix='lin-cow-phase-synthetic-')as td:
 base=Path(td)/'valid';base.mkdir();records=[]
 (base/'provenance.json').write_text(json.dumps(dict(source_restored=True,binaries={'original':{'sha256':'a'},'instrumented':{'sha256':'b'}})))
 for repeat in range(1,7):
  for role in roles:
   group=base/f'{repeat:02}-{role}';group.mkdir()
   for case in cases:
    name=case.replace('/','_');output=group/name;output.mkdir();value={'original':1000,'control':1010,'disabled':1100,'traced':1210}[role]
    data=dict(status='complete',cases=[{'id':case}],observations=[dict(case=case,operations=1,metric='wall',value=str(value))for _ in range(8)])
    (output/'run.json').write_text(json.dumps(data));(group/(name+'-resource.txt')).write_text('Maximum resident set size (kbytes): 123\n')
    records.append(dict(round=repeat,mode=role,case=case,exit_code=0,binary_sha256='a'if role in ['original','control']else'b',peak_rss_kib=123))
    if role=='traced':
     rollback='rollback'in case;calls=[1]*7+([1,2,2,1]if rollback else[0]*4);ns=[10]*7+([20]*4 if rollback else[0]*4)
     trace=dict(phase_names=['positions', 'cas', 'effective_patch', 'guards_fk_undo', 'live_mutation', 'forward_indexes', 'forward_fts_hnsw_maps_output', 'rollback_delta', 'rollback_indexes', 'rollback_fts', 'rollback_maps'],timing_qualified=False,records=[dict(wall_ns=1000,phase_calls=calls,phase_ns=ns,payload_bytes=0 if rollback else 64)for _ in range(12)])
     (group/(name+'-trace.json')).write_text(json.dumps(trace))
 (base/'process-status.json').write_text(json.dumps(records))
 def analyze(directory):return subprocess.run(['python3',str(p/'analyze.py'),str(directory)],capture_output=True,text=True)
 result=analyze(base);assert result.returncode==0,result.stderr
 q=json.loads((base/'qualification.json').read_text());assert q['processes']==96 and q['observations']==768 and q['source_candidate_qualified'] is False and q['full_objective_achieved'] is False
 summary=json.loads((base/'summary.json').read_text());assert len(summary)==4
 for case,row in summary.items():
  assert math.isclose(row['aa_absolute_percent'],1.0) and math.isclose(row['disabled_over_original_ratio'],1.1)and math.isclose(row['traced_over_disabled_ratio'],1.1)
  assert row['trace_records']==72 and row['payload_bytes']==(0 if 'rollback'in case else 64)
  assert row['same_invocation_phase_percent']['rollback_indexes']==(2 if 'rollback'in case else 0)
 checks.append(dict(check='complete-synthetic-matrix',passes=True))
 for mutation in ['missing-process','negative-wall','nan-wall','wrong-rss','missing-forward-index','unexpected-disabled-trace','wrong-rollback-phase']:
  target=Path(td)/mutation;shutil.copytree(base,target)
  if mutation=='missing-process':(target/'process-status.json').write_text(json.dumps(records[:-1]))
  elif mutation in ['negative-wall','nan-wall']:
   f=target/'01-original'/cases[0].replace('/','_')/'run.json';data=json.loads(f.read_text());data['observations'][0]['value']='-1'if mutation=='negative-wall'else'NaN';f.write_text(json.dumps(data))
  elif mutation=='wrong-rss':(target/'01-original'/(cases[0].replace('/','_')+'-resource.txt')).write_text('Maximum resident set size (kbytes): 124\n')
  elif mutation=='unexpected-disabled-trace':(target/'01-disabled'/(cases[0].replace('/','_')+'-trace.json')).write_text('{}')
  else:
   case=cases[0]if mutation=='missing-forward-index'else cases[3];f=target/'01-traced'/(case.replace('/','_')+'-trace.json');data=json.loads(f.read_text())
   if mutation=='missing-forward-index':data['records'][0]['phase_calls'][5]=0
   else:data['records'][0]['phase_calls'][8]=0
   f.write_text(json.dumps(data))
  result=analyze(target);assert result.returncode!=0,mutation;checks.append(dict(check=mutation,rejected=True))
(p/'synthetic-validation.json').write_text(json.dumps(dict(synthetic_only=True,actual_timing_complete=False,checks=checks),indent=2)+'\n');print('Synthetic matrix and seven corruption checks passed')
