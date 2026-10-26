from pathlib import Path
import json
r=Path(__file__).resolve().parent;a=json.loads((r/'audit.json').read_text());env=json.loads((r/'environment.json').read_text())
lines=['# Accepted persistent-worker core — available-peer refresh','',
'2026-10-26, frozen source `b7195e2`. Fresh release `peer_bench` built from the accepted core in its managed worktree. No production source edits. Full source/helper/binary hashes, compiler, raw process reports, versions, image IDs/digests/architecture and server settings are retained. Source hashes checked before/after; benchmark helpers unchanged. Builds completed before measurement.', '',
'## Scope and results','',
'Warm prepared/API read cases at1k/10k/100k, three independently executed processes per size with rotated engine order and12 samples. Values/columns/multiplicity validated against independently generated expectations; order is not compared. Native insertion uses fresh fixtures and exact six-field readback outside timing; Lin includes default embeddings/FTS. Setup, validation, teardown and Lin JSON IPC are outside timers.', '',
'**Read:108/108 aggregate case-size-peer wins,324/324 within-process comparisons positive. Native:15/15 aggregate wins,44/45 within-process comparisons positive.** Read minimum1.723× (100k filtered join versus DuckDB); native minimum1.070× (100k versus DuckDB). These are medians of three process medians under the [benchmark contract](../../../docs/benchmark-contract.md), not universal engine superiority.', '',
'Critical retained outlier: native100k process2 Lin143.974ms /DuckDB102.028ms, a loss. Processes1/3 Lin95.346/93.612ms versus DuckDB101.045/102.041ms. Aggregate Lin95.346ms wins, but this three-process sample does not prove tail stability. No causal explanation is inferred and no retry removes this observation.', '',
'## Versions and ownership','',
', '.join(f'{k}:{v}' for k,v in a['versions'].items())+'.', '',
'Three newly created ARM server-image containers on dedicated localhost ports55433/53307/27028 used owned tmpfs benchmark storage. Actual server durability options are in environment.json. Unique benchmark fixtures and unrelated sentinel objects checked after every size; all sentinels unchanged, no linbench_ objects left. Sentinels removed after each mode. Only the three recorded owned container IDs were stopped and removed; cleanup exits all0. Existing containers were not reused.', '',
'## Interpretation','',
'Lin uses its prepared Rust API. SQLite/DuckDB/pandas use Python APIs; PostgreSQL/MySQL/MongoDB include their drivers, network round trips and result transfer. Read setup and materialization contracts are fixed; no isolated engine CPU comparison is claimed. Pandas is in-memory and appears only in the read matrix.', '',
'Native APIs: Lin prepared Db::empty; SQLite executemany+commit in memory; DuckDB INSERT SELECT from a registered DataFrame; PostgreSQL COPY+commit; MySQL multi-row executemany+commit; MongoDB ordered insert_many with acknowledged w=1,j=false. Server storage is tmpfs. These are successful native API comparisons, not identical durability/failure-atomicity contracts. They do not replace the separate native Rust/Full durable SQLite gate.', '',
'Read4536 timing values are calibrated batch averages, not individual-operation latency samples; native648 values use fresh insertion fixtures. Three process repetitions, rather than every timed iteration, define replication. No new baseline/candidate performance improvement is attributed to the persistent worker by this refresh; native/read paths do not use it.', '',
'MSSQL/Kusto endpoint-presence flags remain false; neither peer was measured and neither counts as a win. The earlier Full durable10k win is separately documented in the [WAL full-engine report](../2026-10-26-wal-persistent-engine/report.md). Full durable1k still loses SQLite. The complete eight-peer objective remains active and unmet.', '']
for result in a['results']:
 lines+=['## '+result['mode']+' matrix','', '| Rows | Case | Peer | Lin ms | Peer ms | Aggregate speedup | Positive processes |','|---:|---|---|---:|---:|---:|---:|']
 for x in result['comparisons']:lines.append(f"| {x['rows']} | {x['case']} | {x['peer']} | {x['lin_ns']/1e6:.6f} | {x['peer_ns']/1e6:.6f} | {x['speedup']:.3f}× | {x['positive_processes']}/3 |")
 lines.append('')
(r/'report.md').write_text('\n'.join(lines))
