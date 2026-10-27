from pathlib import Path
import json,hashlib,re
p=Path(__file__).resolve().parent;load=lambda n:json.loads((p/n).read_text());h=lambda f:hashlib.sha256(Path(f).read_bytes()).hexdigest();m=load('test-status.json');mut=load('mutant-status.json')
assert m['exit_code']==0 and m['source_restored']and not m['performance_qualified']
for f,digest in m['source_sha256'].items():assert h(f)==digest,f
assert h(p/'candidate-persist.rs')==m['candidate_sha256']and h(p/'oracle.rs')==m['oracle_sha256']
counts=[tuple(map(int,x))for x in re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', (p/'tests.log').read_text())];assert counts and sum(x[0]for x in counts)>=409 and all(x[1]==0 for x in counts)
log=(p/'tests.log').read_text();assert 'linux_keep_size_model::bounded_geometry_never_reserves_historical_sparse_prefix'in log and 'linux_keep_size_model::independent_wide_arithmetic_boundary_model'in log
assert mut['expected_failure']and mut['runtime_assertion_caught']and mut['exit_code']!=0 and mut['linux_backend_executed']==m['linux_backend_executed']and 'test result: FAILED'in(p/'mutant.log').read_text()and 'error[E'not in(p/'mutant.log').read_text()
def events(text,marker):
 return [json.JSONDecoder().raw_decode(part)[0]for part in text.split(marker)[1:]]
backend=False
if m['linux_backend_executed']:
 assert sum(x[0]for x in counts)>=413
 names=['actual_keep_size_preserves_eof_cursor_prefix_and_reports_support','read_only_hint_failure_leaves_ordinary_io_and_file_unchanged','reservation_preserves_full_frames_replay_export_and_truncate_append','db_open_seed_rollback_checkpoint_reopen_and_follower_keep_full_values']
 assert all('linux_keep_size_oracle::'+n in log for n in names)
 records=events(log,'LIN_KEEP_SIZE_PROBE ');assert len(records)==2 and {r['case']for r in records}=={'physical','db_cycle'}
 physical=next(r for r in records if r['case']=='physical');assert physical['reserved']and physical['grown']and physical['state']['logical_bytes']==6 and physical['state']['allocated_bytes']>0
 cycle=next(r for r in records if r['case']=='db_cycle');assert cycle['rows']==1000
 for key in ['opened','compacted','closed']:assert cycle[key]['logical_bytes']==0 and cycle[key]['allocated_bytes']==0,(key,cycle[key])
 after=cycle['after_small_commit'];assert after['logical_bytes']>0 and after['allocated_bytes']>=after['logical_bytes']+256*1024
 backend=True
if(p/'container-status.json').exists():
 c=load('container-status.json');assert c['exit_code']==0 and c['standalone_backend_only']and not c['performance_qualified']and c['candidate_sha256']==m['candidate_sha256']and c['helper_sha256']==h(p/'helper.rs')
 for f,digest in c['inputs_sha256'].items():assert h(p/'container-input'/f)==digest
 tests=(p/'container-results/tests.log').read_text();assert '6 passed; 0 failed;'in tests
 records=events(tests,'LIN_KEEP_NATIVE ');assert len(records)==3 and {r['case']for r in records}=={'eof_cursor','sparse_future','same_eof_truncate'}
 eof=next(r for r in records if r['case']=='eof_cursor');assert eof['logical']==6 and eof['position']==2 and eof['allocated_bytes']>0
 sparse=next(r for r in records if r['case']=='sparse_future');assert sparse['logical']==16777223 and sparse['allocated_after']<=sparse['allocated_before']+2*1024*1024
 truncated=next(r for r in records if r['case']=='same_eof_truncate');assert truncated['allocated_before']==1048576 and truncated['allocated_after']==0
 assert int((p/'container-results/mutant-exit-code.txt').read_text())!=0 and 'test result: FAILED'in(p/'container-results/mutant.log').read_text()
print('PASS:',sum(x[0]for x in counts),'default release tests;',sum(x[2]for x in counts),'ignored; range/kernel mutants checked; full Linux integration:',backend,'; no latency qualification')
