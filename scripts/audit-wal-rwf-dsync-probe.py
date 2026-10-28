#!/usr/bin/env python3
"""Independently audit frozen WAL I/O observations, not production DB qualification."""
from pathlib import Path
import argparse,json,subprocess,hashlib,itertools,statistics,math
ROOT=Path(__file__).resolve().parents[1]
HEAD='c3f735b7a3c155c344dd290b9b8b2a8077be208e'
INPUT='benches/results/2026-10-28-wal-rwf-dsync-probe'
def digest(b):return hashlib.sha256(b).hexdigest()
def git(name):return subprocess.check_output(['git','show',HEAD+':'+name],cwd=ROOT)
def audit(parent):
 raw=parent/'wal-rwf-dsync-probe';run=json.loads((parent/'run-metadata.json').read_text());assert run.get('headSha',run.get('head_sha'))==HEAD and run['status']=='completed'and run['conclusion']in ['success','failure']
 manifest=git(INPUT+'/frozen-inputs.json');assert(raw/'frozen-inputs.json').read_bytes()==manifest
 for name,sha in json.loads(manifest).items():
  data=git(name);assert digest(data)==sha
  if name.startswith(INPUT+'/'):assert(raw/name[len(INPUT)+1:]).read_bytes()==data,name
 source=json.loads(git(INPUT+'/source-input.json'));prov=json.loads((raw/'provenance.json').read_text())
 assert prov['source_commit']==HEAD and prov['source_restored']and prov['production_source_changed']is False and prov['performance_qualified']is False and prov['unit_test_exit']==0
 assert prov['gate_sha256']==digest(git(INPUT+'/gate.rs')) and prov['source_sha256']==source['source_sha256']
 assert 'test result: ok. 4 passed;'in(raw/'unit-tests.log').read_text()
 assert 'linux'in prov['platform'].lower()and 'x86_64'in prov['platform']
 assert prov['binary']['sha256']and 'FSTYPE'in(raw/'linux-environment.txt').read_text()
 cases=[(s,h,n)for s,hs in [('full',['scalar','doc_update','doc_delete','doc_batch']),('normal',['scalar','doc_update'])]for h in hs for n in [1,32]]
 fingerprints={};medians={}
 def inspect(stem,c,smoke):
  meta=json.loads(stem.with_suffix('.meta.json').read_text());assert meta['coordinate']==c and meta['exit']==0 and math.isfinite(meta['wall_seconds'])and meta['wall_seconds']>0
  data=stem.with_suffix('.jsonl').read_bytes();assert digest(data)==meta['stdout_sha256'];rows=[json.loads(line)for line in data.splitlines()];assert len(rows)==(1 if smoke else 8)
  cmd=['/tmp/lin-wal-rwf-dsync-probe','--mode',c['mode'],'--sync',c['sync'],'--shape',c['shape'],'--commits',str(c['commits'])]+(['--smoke']if smoke else[]);assert meta['command']==cmd
  for sample,r in enumerate(rows):
   assert r['sample']==sample and r['correctness_only']==smoke and all(r[k]==c[k]for k in ['sync','shape','commits','mode'])
   assert all(r[k]is True for k in ['exact_bytes','complete_replay_model','generation_verified','replay_canonical_bits_verified','corruption_rejected','prefix_synced_before_timing'])
   n=c['commits'];assert len(r['frame_bytes'])==n and all(x>0 for x in r['frame_bytes'])and sum(r['frame_bytes'])==r['target_bytes']and r['prefix_bytes']>0
   assert r['logical_rows']==(32-n if c['shape']=='doc_delete'else 32)
   calls=r['calls'];assert all(isinstance(v,int)and v>=0 for v in calls.values())
   if c['mode']=='candidate'and c['sync']=='full':assert calls['pwritev2']>=n and calls['write_all']==calls['sync_data']==0
   else:assert calls['pwritev2']==0 and calls['write_all']==n and calls['sync_data']==(n if c['sync']=='full'else 0)
   fp=[r[k]for k in ['prefix_bytes','prefix_crc32_ieee','target_bytes','stream_crc32_ieee','frame_bytes']];key=(c['sync'],c['shape'],n)
   if key in fingerprints:assert fp==fingerprints[key]
   fingerprints[key]=fp
   if smoke:assert r['ns_per_commit']is None
   else:assert isinstance(r['ns_per_commit'],(float,int))and math.isfinite(r['ns_per_commit'])and r['ns_per_commit']>0
  return statistics.median(r['ns_per_commit']for r in rows)if not smoke else None
 for sync,shape,n in cases:
  for role in ['baseline','candidate']:inspect(raw/'linux-smoke'/f'{sync}-{shape}-{n}-{role}',dict(sync=sync,shape=shape,commits=n,mode=role),True)
 expected=set()
 for series,pair,(sync,shape,n),role in itertools.product(range(2),range(6),cases,['baseline','control','candidate']):
  name=f'{series}-{pair}-{sync}-{shape}-{n}-{role}';expected.add(name+'.jsonl');c=dict(series=series,pair=pair,sync=sync,shape=shape,commits=n,mode=role)
  medians[series,pair,sync,shape,n,role]=inspect(raw/'raw'/name,c,False)
 assert {f.name for f in(raw/'raw').glob('*.jsonl')}==expected
 windows=[]
 for series,(sync,shape,n)in itertools.product(range(2),cases):
  gains=[];aa=[]
  for pair in range(6):
   b,a,v=[medians[series,pair,sync,shape,n,r]for r in ['baseline','control','candidate']];gains.append(100*(1-v/b));aa.append(abs(100*(1-a/b)))
  gain=statistics.median(gains);noise=statistics.median(aa);passed=(all(v>0 for v in gains)and gain>noise)if sync=='full'else gain>=-max(2,noise)
  windows.append(dict(series=series,sync=sync,shape=shape,commits=n,gain_percent=gain,aa_percent=noise,positive_pairs=sum(v>0 for v in gains),pair_gains=gains,passes=passed))
 expected_q=dict(processes=432,observations=3456,windows=windows,io_gate_passes=all(w['passes']for w in windows),production_qualified=False,full_objective_achieved=False)
 assert json.loads((raw/'qualification.json').read_text())==expected_q
 return dict(audit_complete=True,run_head=HEAD,**expected_q,linux_smokes=24,failed_full_windows=sum(not w['passes']for w in windows if w['sync']=='full'),failed_control_windows=sum(not w['passes']for w in windows if w['sync']=='normal'))
if __name__=='__main__':
 parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('result_directory',type=Path);args=parser.parse_args();result=audit(args.result_directory.resolve());(args.result_directory/'frozen-audit.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))
