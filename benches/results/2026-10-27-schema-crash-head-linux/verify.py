from pathlib import Path
import json,re,hashlib,subprocess
p=Path(__file__).resolve().parent;root=p.parents[2];metadata=json.loads((p/'run-metadata.json').read_text());assert metadata['status']=='completed'and metadata['conclusion']=='success'and metadata['headSha']=='9fdbe74649966442306e9f859241e9989cbbb443'
log=re.sub(r'\x1b\[[0-9;]*m','',(p/'schema-crash-head-linux/tests.log').read_text());counts=[tuple(map(int,row))for row in re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',log)];assert[sum(row[i]for row in counts)for i in range(3)]==[430,0,2]
names=['autocommit_schema_crash_without_snapshot','transaction_schema_crash_without_snapshot','autocommit_schema_crash_after_checkpoint','transaction_schema_crash_after_checkpoint','modeled_new_snapshot_supersedes_stale_head','modeled_head_catalog_is_checked_at_its_wal_generation','corrupt_head_at_checkpoint_generation_still_rejected','corrupt_stale_head_before_schema_wal_still_rejected','unverified_snapshot_cannot_override_stale_head','schema_crash_writer_child']
assert all('test '+name+' ... ok'in log for name in names)
probes=[json.JSONDecoder().raw_decode(part)[0]for part in log.split('SCHEMA_HEAD_PROBE ')[1:]];assert len(probes)==4 and {(r['transaction'],r['prior_checkpoint'])for r in probes}=={(a,b)for a in [True,False]for b in [True,False]}and all(r['reopen_error']is None for r in probes)
manifest=json.loads((root/'benches/results/2026-10-27-schema-crash-head/candidate-source.json').read_text())
for name,digest in manifest.items():assert hashlib.sha256(subprocess.check_output(['git','show',metadata['headSha']+':'+name],cwd=root)).hexdigest()==digest,name
print('PASS: Linux430 passed /2 ignored;all10 schema/publication/corruption regressions and4 clean crash-reopen probes;exact frozen source;no performance claim')
