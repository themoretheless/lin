from pathlib import Path
import subprocess,json,itertools,statistics,math,time,platform,sys,hashlib
P=Path(__file__).resolve().parent
CASES=[(s,h,n) for s,hs in [('full',['scalar','doc_update','doc_delete','doc_batch']),('normal',['scalar','doc_update'])] for h in hs for n in [1,32]]
ROLES=['baseline','control','candidate']
def coordinates():
 for series in range(2):
  for pair in range(6):
   cases=CASES[(series*6+pair)%12:]+CASES[:(series*6+pair)%12]
   for i,(sync,shape,commits) in enumerate(cases):
    roles=ROLES[(series+pair+i)%3:]+ROLES[:(series+pair+i)%3]
    for role in roles:yield dict(series=series,pair=pair,sync=sync,shape=shape,commits=commits,mode=role)
def key(c):return '-'.join(str(c[x])for x in ['series','pair','sync','shape','commits','mode'])
def check(rows,c,smoke=False):
 assert len(rows)==(1 if smoke else 8)
 for i,r in enumerate(rows):
  assert all(r[k]==c[k]for k in ['sync','shape','commits','mode']) and r['sample']==i and r['correctness_only']==smoke
  assert all(r[k] is True for k in ['exact_bytes','complete_replay_model','generation_verified','replay_canonical_bits_verified','corruption_rejected','prefix_synced_before_timing'])
  assert r['target_bytes']==sum(r['frame_bytes']) and len(r['frame_bytes'])==c['commits']
  assert r['logical_rows']==(32-c['commits'] if c['shape']=='doc_delete'else 32)
  calls=r['calls'];n=c['commits']
  if c['mode']=='candidate'and c['sync']=='full':assert calls['pwritev2']>=n and calls['write_all']==calls['sync_data']==0
  else:assert calls['pwritev2']==0 and calls['write_all']==n and calls['sync_data']==(n if c['sync']=='full'else 0)
  if smoke:assert r['ns_per_commit'] is None
  else:assert isinstance(r['ns_per_commit'],(float,int))and math.isfinite(r['ns_per_commit'])and r['ns_per_commit']>0
 return statistics.median(r['ns_per_commit'] for r in rows)if not smoke else None
def analyze(root):
 records={};fingerprints={}
 for c in coordinates():
  stem=root/'raw'/key(c);meta=json.loads(stem.with_suffix('.meta.json').read_text());assert meta['coordinate']==c and meta['exit']==0 and meta['wall_seconds']>0
  data=stem.with_suffix('.jsonl').read_bytes();assert hashlib.sha256(data).hexdigest()==meta['stdout_sha256']
  rows=[json.loads(x)for x in data.splitlines()];records[key(c)]=check(rows,c)
  for r in rows:
   k=(c['sync'],c['shape'],c['commits']);fp=[r[x]for x in ['prefix_bytes','prefix_crc32_ieee','target_bytes','stream_crc32_ieee','frame_bytes']]
   if k in fingerprints:assert fingerprints[k]==fp
   fingerprints[k]=fp
 assert len(records)==432
 windows=[]
 for series,(sync,shape,commits)in itertools.product(range(2),CASES):
  gains=[];noise=[]
  for pair in range(6):
   vals=[records[key(dict(series=series,pair=pair,sync=sync,shape=shape,commits=commits,mode=role))]for role in ROLES]
   b,a,v=vals;gains.append(100*(1-v/b));noise.append(abs(100*(1-a/b)))
  gain=statistics.median(gains);aa=statistics.median(noise)
  passed=all(g>0 for g in gains)and gain>aa if sync=='full'else gain>=-max(2,aa)
  windows.append(dict(series=series,sync=sync,shape=shape,commits=commits,gain_percent=gain,aa_percent=aa,positive_pairs=sum(g>0 for g in gains),pair_gains=gains,passes=passed))
 return dict(processes=432,observations=3456,windows=windows,io_gate_passes=all(w['passes']for w in windows),production_qualified=False,full_objective_achieved=False)
def execute(c,stem,smoke=False):
 cmd=['/tmp/lin-wal-rwf-dsync-probe','--mode',c['mode'],'--sync',c['sync'],'--shape',c['shape'],'--commits',str(c['commits'])]+(['--smoke']if smoke else [])
 start=time.monotonic();r=subprocess.run(cmd,capture_output=True,timeout=180);wall=time.monotonic()-start
 stem.with_suffix('.jsonl').write_bytes(r.stdout);stem.with_suffix('.stderr').write_bytes(r.stderr)
 stem.with_suffix('.meta.json').write_text(json.dumps(dict(coordinate=c,command=cmd,exit=r.returncode,wall_seconds=wall,stdout_sha256=hashlib.sha256(r.stdout).hexdigest()),indent=2)+'\n')
 assert r.returncode==0,(c,r.stderr.decode(errors='replace'))
 check([json.loads(x)for x in r.stdout.splitlines()],c,smoke)
if __name__=='__main__':
 action=sys.argv[1]
 if action in ['smoke','run']:
  assert sys.platform=='linux'and platform.machine()=='x86_64'
  dest=P/('linux-smoke'if action=='smoke'else 'raw');dest.mkdir(exist_ok=False)
  if action=='smoke':
   for sync,shape,commits in CASES:
    for mode in ['baseline','candidate']:
     c=dict(sync=sync,shape=shape,commits=commits,mode=mode);execute(c,dest/f'{sync}-{shape}-{commits}-{mode}',True)
  else:
   for i,c in enumerate(coordinates()):execute(c,dest/key(c));print(f'{i+1}/432 {key(c)}',flush=True)
 elif action=='analyze':
  result=analyze(P);(P/'qualification.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result))
 else:raise ValueError(action)
