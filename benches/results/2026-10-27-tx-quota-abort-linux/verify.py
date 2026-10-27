from pathlib import Path
import json,re,hashlib,subprocess
p=Path(__file__).resolve().parent;root=p.parents[2];m=json.loads((p/'run-metadata.json').read_text());assert m['status']=='completed'and m['conclusion']=='success'and m['headSha']=='edf799b8b9cda3a6f70707d6d1ca2eb3014b52a1'
log=re.sub(r'\x1b\[[0-9;]*m','',(p/'transaction-quota-abort-linux/tests.log').read_text());counts=[tuple(map(int,r))for r in re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',log)];assert sum(r[0]for r in counts)==420 and sum(r[1]for r in counts)==0 and sum(r[2]for r in counts)==2
names=['mutation_quota_rejection_prevents_later_commit','journal_quota_rejection_prevents_later_commit','quota_failure_rolls_back_already_accepted_packs','execution_failure_prevents_prepared_run_and_commit','explicit_rollback_and_drop_remain_available_after_failed_run','abrupt_process_exit_recovers_all_or_none_of_interactive_transaction','every_truncated_commit_frame_leaves_follower_unchanged']
assert all('test '+name+' ... ok'in log for name in names)
probes=[json.JSONDecoder().raw_decode(part)[0]for part in log.split('TX_QUOTA_PROBE ')[1:]];assert len(probes)==2 and {r['journal']for r in probes}=={True,False}
assert all(r['generation']==1 and r['wing']=='rag'and r['wal_after_rejected_write_bytes']==0 and 'aborted'in r['commit_error']for r in probes)
manifest=json.loads((root/'benches/results/2026-10-27-tx-quota-abort/candidate-source.json').read_text())
for name,digest in manifest.items():assert hashlib.sha256(subprocess.check_output(['git','show',m['headSha']+':'+name],cwd=root)).hexdigest()==digest,name
print('PASS: Linux420 passed /2 ignored;all quota/CAS/parse abort and WAL crash regressions;both rejected writes leave original values/generation/no new WAL;exact frozen source')
