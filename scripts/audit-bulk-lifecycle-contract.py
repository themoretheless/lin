#!/usr/bin/env python3
"""Validate the archived bulk timing boundary without remeasuring or trimming data."""
from pathlib import Path
import json,subprocess,hashlib,itertools,re
ROOT=Path(__file__).resolve().parents[1];HEAD='36666994a36ac532122a0de2cac3ad12d869d8ec';P=ROOT/'benches/results/2026-10-28-cow-cas-full-engine-linux';raw=P/'cow-cas-full-engine';a=json.loads((P/'independent-audit.json').read_text());assert a['audit_complete']and a['processes']==2412
expected='fresh input per operation; input setup/drop excluded; output drop included; chunks of <=64'
records=[]
for round,role,n in itertools.product(range(1,13),['baseline','control','candidate'],[1000,10000]):
 label='1k'if n==1000 else'10k';case=f'compare/insert_bulk_{label}/lin';f=raw/f'{round:02}-{role}'/case.replace('/','_')/'run.json';data=f.read_bytes();q=json.loads(data);assert q['status']=='complete'and len(q['cases'])==1 and q['cases'][0]['id']==case and len(q['observations'])==8
 c=q['cases'][0]['contract'];assert c['lifecycle']==expected and c['param.rows']==str(n)and c['samples']=='8'and c['warmup_ns']=='0'
 records.append(dict(round=round,role=role,case=case,run_sha256=hashlib.sha256(data).hexdigest(),lifecycle=c['lifecycle']))
assert len(records)==72
lock=subprocess.check_output(['git','show',HEAD+':Cargo.lock'],cwd=ROOT,text=True);airbug=re.search(r'name = "airbug-bench".*?(?=\n\[\[package\]\]|\Z)',lock,re.S).group(0);revision=re.search(r'branch=release#([a-f0-9]{40})',airbug).group(1);assert revision=='75b1321b5d7ff2cee983379f539c07726a7d27e5'
compare=subprocess.check_output(['git','show',HEAD+':benches/results/2026-10-28-cow-cas-full-engine/compare.rs'],cwd=ROOT)
assert b'fn fill_lin(ins: &mut LinInsert) -> usize {\n    ins.prepared.run(&mut ins.db).expect("lin insert").done.n\n}'in compare
out=dict(audit_complete=True,engine_head=HEAD,airbug_revision=revision,bulk_processes=72,wall_observations=576,contract=expected,source_compare_sha256=hashlib.sha256(compare).hexdigest(),records=records,performance_claim=False)
(P/'bulk-lifecycle-contract-audit.json').write_text(json.dumps(out,indent=2)+'\n')
(ROOT/'docs/bulk-lifecycle-2026-10-28.md').write_text('''# Bulk benchmark timing boundary

The archived full-engine run40607797376 at frozen36666994a records the same lifecycle for all72Lin bulk processes (1k/10k,12rounds,baseline/control/candidate),576wall observations:

> fresh input per operation; input setup/drop excluded; output drop included; chunks of <=64

Cargo.lock pins airbug-bench75b1321b5d7ff2cee983379f539c07726a7d27e5. Its suite_measure input_batch creates inputs before Instant::now andrecords elapsed time before input destruction. InsideTiming controls output drop; it does not include Db/input teardown. bench_checked performs validation on separate fresh operations outside timing.

The frozen Lin bulk closure invokes fill_lin,which runs the prepared statement andextracts done.n. The temporary execution Handle is destroyed inside fill_lin;theclosure output is usize. Input Db andPrepared destruction remain outside the timer. Bulk setup enables the same default embedder,scalar index andFTS before preparing the insertion.

Therefore profiling Db::drop cannot explain a regression in the recorded bulk timing. A diagnostic must isolate prepared execution andits temporary result/transaction lifetime. It must keep preparation,initial database/index setup,validation andinput teardown outside its measured region. No existing timing observations oracceptance thresholds are changed by this clarification.

The current executor also already skips rows_to_insert_cols for a non-durable insert andmoves large batches into Store while eliding Handle.rows above128rows. Those existing optimizations must not be proposed as new changes. Smaller Row-descriptor traffic orenum dispatch remains an unproven cause of the5–8%bulk regression;the rejected owned-map-template experiment did not establish it.

bulk-lifecycle-contract-audit.json records all72raw hashes. This is a contract audit,not a new performance result oracceptance of any COW candidate.
''')
print('verified all72bulk process contracts /576observations; input setup/drop excluded')
