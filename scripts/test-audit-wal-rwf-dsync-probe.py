"""Synthetic auditor sensitivity only; creates no performance evidence."""
from pathlib import Path
import tempfile,json,importlib.util,sys,hashlib,subprocess,shutil,copy
root=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('rwf_audit',root/'scripts/audit-wal-rwf-dsync-probe.py');a=importlib.util.module_from_spec(spec);spec.loader.exec_module(a)
sys.path.insert(0,str(root/a.INPUT));from matrix import coordinates,key,analyze
with tempfile.TemporaryDirectory(prefix='lin-rwf-audit-synthetic-')as td:
 p=Path(td);raw=p/'wal-rwf-dsync-probe';raw.mkdir();(raw/'raw').mkdir();(raw/'linux-smoke').mkdir()
 (p/'run-metadata.json').write_text(json.dumps(dict(headSha=a.HEAD,status='completed',conclusion='success')))
 for name in json.loads(a.git(a.INPUT+'/frozen-inputs.json')):
  if name.startswith(a.INPUT+'/'):
   f=raw/name[len(a.INPUT)+1:];f.parent.mkdir(parents=True,exist_ok=True);f.write_bytes(a.git(name))
 (raw/'frozen-inputs.json').write_bytes(a.git(a.INPUT+'/frozen-inputs.json'))
 prov=json.loads(a.git(a.INPUT+'/provenance.json'));prov.update(source_commit=a.HEAD,platform='SYNTHETIC linux x86_64');(raw/'provenance.json').write_text(json.dumps(prov));(raw/'unit-tests.log').write_text('SYNTHETIC test result: ok. 4 passed;');(raw/'linux-environment.txt').write_text('SYNTHETIC FSTYPE')
 def record(c,smoke):
  r=json.loads((raw/'local-smoke'/f"{c['sync']}-{c['shape']}-{c['commits']}.json").read_text());r.update(mode=c['mode'],correctness_only=smoke,ns_per_commit=None if smoke else(90 if c['mode']=='candidate'and c['sync']=='full'else 100))
  if c['mode']=='candidate'and c['sync']=='full':r['calls']=dict(write_all=0,sync_data=0,pwritev2=c['commits'],interrupted=0,partial=0)
  rows=[dict(r,sample=i)for i in range(1 if smoke else 8)];data=''.join(json.dumps(v)+'\n'for v in rows).encode()
  stem=raw/('linux-smoke'if smoke else'raw')/(f"{c['sync']}-{c['shape']}-{c['commits']}-{c['mode']}"if smoke else key(c))
  stem.with_suffix('.jsonl').write_bytes(data);cmd=['/tmp/lin-wal-rwf-dsync-probe','--mode',c['mode'],'--sync',c['sync'],'--shape',c['shape'],'--commits',str(c['commits'])]+(['--smoke']if smoke else[])
  stem.with_suffix('.meta.json').write_text(json.dumps(dict(coordinate=c,exit=0,wall_seconds=1,stdout_sha256=hashlib.sha256(data).hexdigest(),command=cmd)))
 for c in coordinates():record(c,False)
 from matrix import CASES
 for sync,shape,n in CASES:
  for mode in ['baseline','candidate']:record(dict(sync=sync,shape=shape,commits=n,mode=mode),True)
 (raw/'qualification.json').write_text(json.dumps(analyze(raw)));assert a.audit(p)['audit_complete']
 def reject(name,mutation):
  paths=list(raw.rglob('*'))+[p/'run-metadata.json'];before={f:f.read_bytes()for f in paths if f.is_file()}
  mutation()
  try:a.audit(p)
  except (AssertionError,FileNotFoundError):pass
  else:raise RuntimeError(name+' accepted')
  for f,data in before.items():f.write_bytes(data)
 def alter_json(f,fn):
  obj=json.loads(f.read_text());fn(obj);f.write_text(json.dumps(obj))
 reject('missing process',lambda:next((raw/'raw').glob('*.jsonl')).unlink())
 reject('summary falsification',lambda:alter_json(raw/'qualification.json',lambda q:q.update(io_gate_passes=False)))
 reject('source modification',lambda:(raw/'gate.rs').write_text('modified'))
 reject('wrong run head',lambda:alter_json(p/'run-metadata.json',lambda q:q.update(headSha='0'*40)))
 reject('incomplete run',lambda:alter_json(p/'run-metadata.json',lambda q:q.update(status='in_progress')))
 reject('negative process duration',lambda:alter_json(next((raw/'raw').glob('*.meta.json')),lambda q:q.update(wall_seconds=-1)))
 reject('missing Linux support smoke',lambda:next((raw/'linux-smoke').glob('*.jsonl')).unlink())
 assert a.audit(p)['io_gate_passes']
print('SYNTHETIC ONLY: independent complete-matrix audit passed; seven corruptions rejected')
