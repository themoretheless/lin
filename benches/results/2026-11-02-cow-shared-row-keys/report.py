from pathlib import Path
import json
p=Path(__file__).resolve().parent
assert json.loads((p/'independent-audit.json').read_text())['audit_complete']
s=json.loads((p/'summary.json').read_text())
lines=['# Shared row keys: local discovery results','','Positive gain means lower candidate latency. AA is the median absolute baseline/control difference. One local six-round series is not Linux/full-matrix qualification.','','| Case | Gain % | AA % | Positive pairs |','|---|---:|---:|---:|']
for case,r in s.items():lines.append(f"| {case} | {r['gain_percent']:+.3f} | {r['aa_absolute_percent']:.3f} | {r['positive_pairs']}/6 |")
lines+=['','Allocation proof: cloning twelve fields takes 3 allocations instead of 15 for both short and long keys. Full release correctness: 462 passed, 2 ignored, including the allocation test.','','The candidate harness adapts only durable_json_row from k.clone() to k.to_string() for the vnext borrowed key API; this JSON conversion occurs in setup/verification. All timed workload code is unchanged. Compiler identity and build-input hashes are checked. DuckDB uses the exact baseline binary under every role label.','','The host process snapshot at startup records concurrent unrelated Rust compilation, from other projects. This shared-host run is exploratory and cannot establish isolated-machine throughput or qualify adoption. The baseline/control measurements quantify observed variation but do not remove this limitation.','','No adoption or all-peer superiority claim follows from this local experiment. SQLite FULL WAL, the six-peer corrected CI run, private MSSQL evidence and the missing real Kusto endpoint remain separate requirements.']
(p/'report.md').write_text('\n'.join(lines)+'\n')
(p/'decision.json').write_text(json.dumps(dict(local_discovery_only=True,linux_qualified=False,full_matrix_qualified=False,adopted=False,all_peer_goal_achieved=False),indent=2)+'\n')
