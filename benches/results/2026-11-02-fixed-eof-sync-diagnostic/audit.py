from pathlib import Path
import hashlib,json,statistics,sys
p=Path(__file__).resolve().parent;root=p.parents[2]
sys.path.insert(0,str(root/'scripts'))
from benchmark_role_balance import schedule,orders_from_records
sha=lambda b:hashlib.sha256(b).hexdigest()
load=lambda name:json.loads((p/name).read_text())
inputs=load('input.json');env=load('environment.json');records=load('process-status.json')
assert load('status.json')==dict(complete=True,processes=36,observations=2304,source_unchanged=True,owned_files_removed=True,performance_qualification=False)
for name,h in inputs['files_sha256'].items():assert sha((root/name).read_bytes())==h,name
assert sha(Path(env['binary']['path']).read_bytes())==env['binary']['sha256']
roles=['append_a','append_b','fixed_sparse_a','fixed_sparse_b','fixed_zeroed_a','fixed_zeroed_b']
plan=schedule([dict(id='point_sync')],roles,rounds=6)
assert len(records)==36 and [{k:r[k]for k in ['round','workload','role']}for r in records]==plan
orders_from_records(records,['point_sync'],roles,'workload',rounds=6)
med={};observations=0
for r in records:
 mode=r['role'].rsplit('_',1)[0];target=p/'owned-files'/f"{r['round']:02}-{r['role']}.bin"
 assert r['command']==[env['binary']['path'],mode,str(target)]and r['exit_code']==0 and r['binary_sha256']==env['binary']['sha256']
 name=f"raw/{r['round']:02}-{r['role']}.json";assert r['raw']==name
 d=load(name);values=d['latencies_ns'];assert len(values)==64 and all(type(v)is int and v>0 for v in values)
 assert d['mode']==mode and d['samples']==64 and d['payload_bytes']==4125 and d['logical_end']==280384
 assert d['physical_bytes']==(280384 if mode=='append'else 8388608)
 assert d['full_readback_verified']and d['owned_file_removed']and not target.exists()
 median=statistics.median(values);assert median==r['median_ns'];med[r['round'],r['role']]=median;observations+=len(values)
assert observations==2304
summary={}
for mode in ['append','fixed_sparse','fixed_zeroed']:
 pairs=[]
 for rnd in range(1,7):
  base=med[rnd,'append_a'];a=med[rnd,mode+'_a'];b=med[rnd,mode+'_b']
  pairs.append(dict(round=rnd,mode_a_ns=a,mode_b_ns=b,append_ns=base,gain_percent=100*(1-a/base),same_mode_control_percent=100*(1-b/a)))
 summary[mode]=dict(gain_percent=statistics.median(r['gain_percent']for r in pairs),same_mode_aa_percent=statistics.median(abs(r['same_mode_control_percent'])for r in pairs),positive_pairs=sum(r['gain_percent']>0 for r in pairs),pairs=pairs)
(p/'audit.json').write_text(json.dumps(dict(audit_complete=True,processes=36,observations=2304,raw_recomputed=True,balanced_order_checked=True,exact_commands_checked=True,binary_hash_checked=True,source_unchanged=True,summary=summary,performance_qualification=False,wal_format_proven=False),indent=2)+'\n')
print(json.dumps(summary,indent=2))
