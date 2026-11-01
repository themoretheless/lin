from pathlib import Path
import subprocess,json,hashlib,re
p=Path(__file__).resolve().parent;m=json.loads((p/'input.json').read_text())
for key in ['source_sha256','tests_sha256']:
 assert all(hashlib.sha256(Path(n).read_bytes()).hexdigest()==h for n,h in m[key].items())
with(p/'full-release.log').open('w')as log:
 r=subprocess.run(['cargo','test','--release','--lib','--tests','--color','never'],stdout=log,stderr=subprocess.STDOUT)
counts=re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored',(p/'full-release.log').read_text())
s=dict(exit_code=r.returncode,release_passed=sum(int(c[0])for c in counts),release_ignored=sum(int(c[2])for c in counts),source_sha256=m['source_sha256'],tests_sha256=m['tests_sha256'],compiler=subprocess.check_output(['rustc','-Vv'],text=True),performance_qualified=False)
(p/'test-status.json').write_text(json.dumps(s,indent=2)+'\n')
assert r.returncode==0 and s['release_passed']==458 and s['release_ignored']==2
for name in ['index::bitmap_empty_oracle::empty_probe_tracks_dense_sparse_and_last_bit_across_promotion_and_growth', 'store::cow_row_oracle::cloned_rows_do_not_alias_mutations_and_serde_stays_map_shaped', 'store::inplace_patch_engine_oracle::held_row_isolated_and_body_hash_fts_maps_restore_on_rollback', 'store::lazy_row_oracle::actual_update_promotes_only_touched_row_and_keeps_held_result', 'store::lazy_row_oracle::deterministic_map_model_across_promotions_and_held_clones', 'store::lazy_row_oracle::representation_preserves_eq_hash_wire_debug_and_snapshot_isolation']:assert 'test '+name+' ... ok' in (p/'full-release.log').read_text(),name
