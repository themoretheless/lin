from pathlib import Path
import hashlib,json,re
p=Path(__file__).resolve().parent;load=lambda n:json.loads((p/n).read_text());h=lambda f:hashlib.sha256(Path(f).read_bytes()).hexdigest();m=load('test-status.json');mut=load('mutant-status.json');cas=load('mutant-validation.json')
assert m['exit_code']==0 and m['source_restored'] and not m['performance_qualified'];assert all(h(f)==d for f,d in m['source_sha256'].items())
s=(p/'full-tests.log').read_text();counts=re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',s);assert [sum(int(r[i])for r in counts)for i in range(3)]==[449,0,2]
for name in ['representation_preserves_eq_hash_wire_debug_and_snapshot_isolation','deterministic_map_model_across_promotions_and_held_clones','cow_rollback_preserves_held_updated_result_and_restores_complete_row','single_cas_each_checks_all_rows_and_retains_held_results','multi_statement_cas_each_still_rejects_prior_writes_and_restores_everything','single_cas_each_unique_failure_rolls_back_complete_rows_and_indexes','repeated_single_cas_each_in_transaction_preserves_commit_and_rollback_contracts']:assert name+' ... ok'in s,name
assert mut['exit_code']==101 and mut['expected_failure']and mut['runtime_assertion_caught']and'test result: FAILED'in(p/'mutant.log').read_text()
assert cas['runtime_rejected']and cas['source_restored']and cas['original_exec_sha256']==h('src/exec.rs')and 'multi_statement_cas_each_still_rejects_prior_writes_and_restores_everything ... FAILED'in(p/'mutant-runtime.log').read_text()
c=load('component-qualification.json');a=load('component-independent-audit.json');cm=load('component-provenance.json')
assert c['measurement_complete']and c['component_gate_passes']and c['processes']==144 and c['observations']==1152
assert a['audit_complete']and a['failed_target_windows']==a['failed_control_windows']==0 and cm['source_sha256']==m['source_sha256']
for case,series_rows in load('component-summary.json').items():
 for series,row in series_rows.items():assert row['passes']and(not row['target']or row['positive_pairs']==6)
for name in ['exec','store','check','index']:assert h(p/('candidate-'+name+'.rs'))==m['source_sha256']['src/'+name+'.rs']
print('Functional audit PASS:449release tests/2ignored;COW Eq and CAS conflict mutants caught;144-process scalar component passed;no full-engine acceptance')
