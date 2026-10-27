from pathlib import Path
import hashlib,json,re
p=Path(__file__).resolve().parent;load=lambda n:json.loads((p/n).read_text());h=lambda f:hashlib.sha256(Path(f).read_bytes()).hexdigest();m=load('test-status.json');mut=load('mutant-status.json')
assert m['exit_code']==0 and m['source_restored'] and not m['performance_qualified'];assert all(h(f)==d for f,d in m['source_sha256'].items())
s=(p/'full-tests.log').read_text();counts=re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',s);assert [sum(int(r[i]) for r in counts) for i in range(3)]==[444,0,2]
for name in ['representation_preserves_eq_hash_wire_debug_and_snapshot_isolation','deterministic_map_model_across_promotions_and_held_clones','cow_rollback_preserves_held_updated_result_and_restores_complete_row']:assert name+' ... ok' in s
assert mut['exit_code']==101 and mut['expected_failure'] and mut['runtime_assertion_caught'] and 'test result: FAILED' in (p/'mutant.log').read_text()
w=load('warm-summary.json');wm=load('warm-provenance.json');assert not w['diagnostic_gate_passes'] and wm['source_sha256']==m['source_sha256'];targets=[r for r in w['results'] if r['metric']=='ns_per_update'];assert len(targets)==6 and all(r['passes'] and r['positive_pairs']==6 for r in targets)
for name,d in wm['binaries']['candidate']['candidate_sources_sha256'].items():assert h(p/('candidate-'+Path(name).name))==d
print('Functional audit PASS:444release tests/2ignored;COW alias/model/held-result and Eq mutant;UPDATE warm windows passed,overall warm gate failed;no full-engine acceptance')
