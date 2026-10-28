from pathlib import Path
import json,tempfile,subprocess,shutil,hashlib,itertools
import importlib.util
root=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('bulk_audit',root/'scripts/audit-bulk-embedding-phases.py');a=importlib.util.module_from_spec(spec);spec.loader.exec_module(a)
p=root/a.INPUT
original={f'src/{n}.rs':a.git(a.INPUT+f'/original-{n}.rs')for n in ['exec','lib','embed']};instrumented={f'src/{n}.rs':a.git(a.INPUT+f'/instrumented-{n}.rs')for n in ['exec','lib','embed']};patches=json.loads(a.git(a.INPUT+'/patches.json'));a.reversible_overlay(original,instrumented,patches)
wrong=dict(instrumented);wrong['src/exec.rs']=wrong['src/exec.rs'].replace(b'self.maybe_embed_rows(collection, &mut built);',b'let _ = &mut built;')
try:a.reversible_overlay(original,wrong,patches)
except AssertionError:pass
else:raise RuntimeError('non-clock operation change accepted')
with tempfile.TemporaryDirectory(prefix='lin-bulk-phase-synthetic-')as td:
 d=Path(td);(d/'analyze.py').write_bytes(a.git(a.INPUT+'/analyze.py'));(d/'provenance.json').write_text(json.dumps(dict(binaries={k:dict(sha256=k)for k in ['original','instrumented']})));records=[];cases=['compare/insert_bulk_1k/lin','compare/insert_bulk_10k/lin'];roles=['original','control','disabled','traced']
 for round,role,case in itertools.product(range(1,7),roles,cases):
  dest=d/f'{round:02}-{role}'/case.replace('/','_');dest.mkdir(parents=True);q=dict(status='complete',cases=[dict(id=case,contract=dict(lifecycle='fresh input per operation; input setup/drop excluded; output drop included; chunks of <=64'))],observations=[dict(case=case,metric='wall',operations=1,value=100)for _ in range(8)]);(dest/'run.json').write_text(json.dumps(q));r=dict(round=round,role=role,case=case,exit=0,elapsed_seconds=1,binary_sha256='original'if role in ['original','control']else'instrumented',trace_sha256=None)
  if role=='traced':
   trace=dict(phase_names=['unused_parent','text_collect','refs','lowercase','feature_hash','normalize','vector_copy','scratch_reset','row_install'],timing_qualified=False,records=[dict(wall_ns=100,phase_calls=[0,1,1]+[1000 if case==cases[0]else 10000]*5+[1],phase_ns=[0]+[5]*8,payload_bytes=0)for _ in range(10)]);data=json.dumps(trace).encode();dest.with_suffix('.trace.json').write_bytes(data);r['trace_sha256']=hashlib.sha256(data).hexdigest()
  records.append(r)
 status=d/'process-status.json';status.write_text(json.dumps(records))
 def run():
  result=subprocess.run(['python3',str(d/'analyze.py')],capture_output=True)
  try:q=a.matrix(d)
  except(AssertionError,FileNotFoundError):assert result.returncode!=0
  else:assert result.returncode==0 and q['audit_complete']
  return result
 assert run().returncode==0;assert json.loads((d/'qualification.json').read_text())['diagnostic_complete']
 f=d/'01-original'/cases[0].replace('/','_')/'run.json';saved=f.read_bytes();f.unlink();assert run().returncode!=0;f.write_bytes(saved);q=json.loads(saved);q['observations'][0]['value']=-1;f.write_text(json.dumps(q));assert run().returncode!=0;f.write_bytes(saved)
 record=next(r for r in records if r['role']=='traced');trace=d/f"{record['round']:02}-traced"/record['case'].replace('/','_');trace=trace.with_suffix('.trace.json');saved=trace.read_bytes();digest=record['trace_sha256']
 for field,value in [('phase_calls',[0]+[1]*8),('phase_ns',[0]+[50]*8)]:
  q=json.loads(saved);q['records'][0][field]=value;data=json.dumps(q).encode();trace.write_bytes(data);record['trace_sha256']=hashlib.sha256(data).hexdigest();status.write_text(json.dumps(records));assert run().returncode!=0;trace.write_bytes(saved);record['trace_sha256']=digest;status.write_text(json.dumps(records))
 assert run().returncode==0
print('SYNTHETIC ONLY: independent48-process audit passes;four raw corruptions andnon-clock source mutation rejected')
