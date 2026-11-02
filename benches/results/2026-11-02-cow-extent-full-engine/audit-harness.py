from pathlib import Path
import hashlib,json,re,difflib
p=Path(__file__).resolve().parent;old=p.parent/'2026-11-02-shared-peer-full-engine';before=(old/'compare.rs').read_text();after=(p/'compare.rs').read_text()
def functions(source):
 result={}
 for m in re.finditer(r'^fn (\w+)\b',source,re.M):
  end=source.index('\n}',m.end())+2
  assert m[1]not in result,m[1];result[m[1]]=source[m.start():end]
 return result
a=functions(before);b=functions(after);allowed={'setup_lin_durable_append_sync','setup_durable_lin_write','verify_durable_lin_write','setup_wal_scalar_control','verify_wal_scalar_control','durable_json_row'}
changed={n for n in a if a[n]!=b[n]};assert changed==allowed,changed;assert set(b)-set(a)=={'open_combined_db'};assert not set(a)-set(b)
assert (p/'cases.json').read_bytes()==(old/'cases.json').read_bytes();assert len(json.loads((p/'cases.json').read_text()))==67
# Every operation and oracle outside the six explicitly listed setup/reopen/JSON changes is byte-identical.
assert a['durable_json_row'].replace('(k.clone(),value)','(k.to_string(),value)')==b['durable_json_row']
for n in ['run_durable_lin_write','run_wal_scalar_control','write_update_lin','write_delete_lin','tx_update_commit_lin','tx_update_rollback_lin','fill_lin_durable_append']:assert a[n]==b[n],n
result=dict(original_harness_sha256=hashlib.sha256(before.encode()).hexdigest(),compiled_harness_sha256=hashlib.sha256(after.encode()).hexdigest(),existing_functions=len(a),byte_identical_functions=len(a)-len(changed),changed_functions=sorted(changed),new_functions=['open_combined_db'],cases_identical=67,timed_operation_changes=0,external_function_changes=0)
(p/'harness-audit.json').write_text(json.dumps(result,indent=2)+'\n');(p/'harness.patch').write_text(''.join(difflib.unified_diff(before.splitlines(True),after.splitlines(True),fromfile='previous-shared-compare.rs',tofile='combined-compare.rs')));print(json.dumps(result))
