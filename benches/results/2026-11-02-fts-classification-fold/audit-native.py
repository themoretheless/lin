from pathlib import Path
import hashlib,json,statistics,sys
p=Path(__file__).resolve().parent;root=p.parents[2];sha=lambda b:hashlib.sha256(b).hexdigest();raw=json.loads((p/'native-process-status.json').read_text());build=json.loads((p/'worker-build.json').read_text());summary=json.loads((p/'native-summary.json').read_text());sys.path.insert(0,str(root/'scripts'));from benchmark_role_balance import role_order
expected=[(s,r,n,role)for s in [1,2]for r in range(1,7)for ci,n in enumerate([1000,10000,100000])for role in role_order(r,ci,['base','cont','cand'])];assert [(o['series'],o['round'],o['rows'],o['role'])for o in raw]==expected and len(raw)==108
for role in ['baseline','candidate']:assert sha(Path(build[role]['path']).read_bytes())==build[role]['sha256']
assert {n for n in build['baseline']['source_sha256']if build['baseline']['source_sha256'][n]!=build['candidate']['source_sha256'][n]}=={'src/fts.rs'}
rows=0
for o in raw:
 s,r,n,role=o['series'],o['round'],o['rows'],o['role'];b=build['candidate'if role=='cand'else'baseline'];assert o['command']==[b['path']]and o['binary_sha256']==b['sha256']and o['request']==dict(insert_rows=n,samples=8,timestamp_ms=1700000000000)
 d=json.loads((p/'native-raw'/f'{s}-{r}-{n}-{role}.json').read_text());assert d['samples_ns']==o['samples_ns']and len(d['samples_ns'])==8 and statistics.median(d['samples_ns'])==o['median_ns'];assert len(d['result'])==n and d['statements']==(n+9999)//10000 and d['statement_rows_limit']==10000
 for i,row in enumerate(d['result']):assert row==[f'd-{i}',f'bench://{i}','rag'if i%2==0 else'sys',f'doc {i} wal note'if i%10==0 else f'doc {i} plain',1700000000000,f'body {i}']
 rows+=n
for series in [1,2]:
 for n in [1000,10000,100000]:
  gains=[];aa=[]
  for rnd in range(1,7):
   v={o['role']:o['median_ns']for o in raw if o['series']==series and o['rows']==n and o['round']==rnd};gains.append((1-v['cand']/v['base'])*100);aa.append(abs(v['cont']/v['base']-1)*100)
  assert summary[f'{series}-{n}']==dict(median_gain_percent=statistics.median(gains),median_aa_percent=statistics.median(aa),positive_pairs=sum(x>0 for x in gains),gains_percent=gains)
assert rows==3996000
(p/'independent-native-audit.json').write_text(json.dumps(dict(processes=108,samples=864,all_six_fields_verified_rows=rows,schedule_commands_binary_and_summary_verified=True,same_compiler_features_and_worker_source=True,only_fts_source_differs=True,performance_qualified=False,external_peer_comparisons=0),indent=2)+'\n')
lines=['# Three-state FTS fold: native insertion discovery','','Same compiler and default+experimental-extent-wal features, same root peer_bench example and immutable worker source. Baseline binary comes from the archived earlier exact-feature functional preflight; all recorded build/source files match candidate except src/fts.rs. Base/control share that exact executable. Candidate changes only normalization plus its cfg(test) Unicode oracle. Existing worker operation timers, chunk quotas, transaction, FTS, embedding and exact data remain intact. Two balanced six-round series:108 processes /864 samples;3996000 full six-field row readbacks independently verified. No local build/test overlap. Unrelated host activity uncontrolled.','','| Series | Rows | Gain % | AA % | Positive pairs |','|---|---:|---:|---:|---:|']
for key,d in summary.items():
 s,n=key.split('-');lines.append(f"| {s} | {n} | {d['median_gain_percent']:+.3f} | {d['median_aa_percent']:.3f} | {d['positive_pairs']}/6 |")
lines+=['','Mixed result.10k improves in6/6 pairs in both series above the corresponding AA medians.100k first series remains below AA and only5/6 positive, second improves6/6;1k is not uniformly improved. No full-source gate, Linux result, external peer victory or production adoption follows. Preserve every window, including1k losses; do not pool with the standalone function prototype or older source candidates. The already-live451fd7a25 full matrix excludes this fold. Next requires exact-source Linux correctness and performance/peer qualification; previous COW/extent acceptance cannot be transferred.']
(p/'native-report.md').write_text('\n'.join(lines)+'\n');print('Independent native audit passed:108 processes,864 samples,3996000 rows')
