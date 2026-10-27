from pathlib import Path
import json,hashlib
p=Path(__file__).resolve().parent;meta=json.loads((p/'allocation-provenance.json').read_text());status=json.loads((p/'allocation-status.json').read_text());summary=json.loads((p/'allocation-summary.json').read_text());assert len(status)==6 and {(r['round'],r['role'])for r in status}=={(i,m)for i in range(1,4)for m in ['baseline','candidate']};raw={}
for r in status:
 assert r['exit_code']==0;rows=[json.loads(l)for l in(p/f'alloc-{r["round"]:02}-{r["role"]}.jsonl').read_text().splitlines()];assert len(rows)==9 and {(x['rows'],x['case'])for x in rows}=={(n,c)for n in [1000,10000,100000]for c in ['wing','title','delete']}
 for row in rows:
  assert row['exact_readback']and row['full_output_validated']and row['handle_drop_included'];raw[r['round'],r['role'],row['rows'],row['case']]=row
for n in [1000,10000,100000]:
 for case in ['wing','title','delete']:
  s=summary['results'][f'{n}/{case}']
  for key in ['allocations','requested_bytes','deallocations']:
   for role in ['baseline','candidate']:
    xs=sorted(raw[i,role,n,case][key]for i in range(1,4));assert s[role][key]==xs[1]
   assert s['difference'][key]==s['candidate'][key]-s['baseline'][key]
assert meta['source_restored']and all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in meta['source_sha256'].items())
assert summary['processes']==6 and summary['observations']==54 and summary['latency_measured']is False and summary['rss_measured']is False
print('Independent allocation audit PASS: 6 processes, 54 complete-output checked observations')
