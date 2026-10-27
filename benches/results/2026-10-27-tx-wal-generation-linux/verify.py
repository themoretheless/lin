from pathlib import Path
import hashlib,json,re,subprocess
p=Path(__file__).resolve().parent;root=p.parents[2]
run=json.loads((p/'run-metadata.json').read_text());assert run['status']=='completed'and run['conclusion']=='success'and run['headSha']=='046226580717bec1f9a7af9e44d3ef5cc2134fd2'
job=run['jobs'][0];assert job['conclusion']=='success'and all(s['conclusion']=='success'for s in job['steps'])
log=re.sub(r'\x1b\[[0-9;]*m','',(p/'transaction-wal-generation-linux/tests.log').read_text())
counts=[tuple(map(int,r))for r in re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',log)]
assert counts and sum(r[0]for r in counts)==415 and sum(r[1]for r in counts)==0 and sum(r[2]for r in counts)==2
names=['in_memory_multi_write_commit_advances_generation_once','dropped_and_read_only_transactions_preserve_committed_generation','multi_statement_commit_is_one_contiguous_atomic_frame','rollback_keeps_next_wal_generation_contiguous','every_truncated_commit_frame_leaves_follower_unchanged','checkpoint_failure_after_wal_publication_keeps_transaction_committed','crash_writer_child','abrupt_process_exit_recovers_all_or_none_of_interactive_transaction']
assert all('test '+name+' ... ok'in log for name in names)
probes=[json.loads(line.split('GEN_PROBE ',1)[1])for line in log.splitlines()if line.startswith('GEN_PROBE ')]
assert len(probes)==2 and {r['case']for r in probes}=={'rollback','multi_commit'}
assert all(r['base']==1 and r['current']==2 and r['wal_generations']==[2]and r['follower_error']is None for r in probes)
assert next(r for r in probes if r['case']=='rollback')['after_rollback']==1
manifest=json.loads((root/'benches/results/2026-10-27-tx-wal-generation/candidate-source.json').read_text())
for name,digest in manifest.items():
 data=subprocess.check_output(['git','show',run['headSha']+':'+name],cwd=root);assert hashlib.sha256(data).hexdigest()==digest,name
print('PASS: Linux 415 passed /2 ignored; all eight transaction/WAL regression tests and both generation probes; exact frozen source hashes; no performance qualification')
