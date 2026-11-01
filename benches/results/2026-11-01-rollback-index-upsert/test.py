from pathlib import Path
import subprocess,json,hashlib,re
p=Path(__file__).resolve().parent;m=json.loads((p/'input.json').read_text())
for n,h in m['source_sha256'].items():assert hashlib.sha256(Path(n).read_bytes()).hexdigest()==h,n
assert hashlib.sha256(Path('tests/partial_index_mutation.rs').read_bytes()).hexdigest()==m['test_sha256']
with(p/'release-tests.log').open('w')as log:r=subprocess.run(['cargo','test','--release','--lib','--tests','--color','never'],stdout=log,stderr=subprocess.STDOUT)
log=(p/'release-tests.log').read_text();counts=re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored',log)
status=dict(exit_code=r.returncode,release_passed=sum(int(c[0])for c in counts),release_ignored=sum(int(c[2])for c in counts),source_sha256=m['source_sha256'],test_sha256=m['test_sha256'],compiler=subprocess.check_output(['rustc','-Vv'],text=True),performance_qualified=False)
(p/'test-status.json').write_text(json.dumps(status,indent=2)+'\n');assert r.returncode==0 and status['release_passed']==459 and status['release_ignored']==2
for name in ['rollback_upsert_restores_partial_and_unique_index_entries','store::cow_row_oracle::cloned_rows_do_not_alias_mutations_and_serde_stays_map_shaped','store::lazy_row_oracle::representation_preserves_eq_hash_wire_debug_and_snapshot_isolation']:assert 'test '+name+' ... ok'in log,name
