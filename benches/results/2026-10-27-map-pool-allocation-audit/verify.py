from pathlib import Path
import hashlib,json
p=Path(__file__).resolve().parent;m=json.loads((p/'provenance.json').read_text());assert m['source_restored']
assert hashlib.sha256((p/'probe.rs').read_bytes()).hexdigest()==m['probe_sha256']
for f,h in m['source_sha256'].items():assert hashlib.sha256(Path(f).read_bytes()).hexdigest()==h,f
expected={'clone_drop','clone_from_held_destination','tls_acquire_fill_release_warm'}
for i in range(1,4):
 rows=json.loads((p/f'run-{i}.json').read_text());assert len(rows)==10
 cold=rows[0];assert cold['case']=='tls_acquire_fill_release_cold'and cold['operations']==1 and cold['allocations']==11 and cold['requested_bytes']==2118 and cold['deallocations']==10 and cold['deallocated_bytes']==582
 warm=rows[1:];assert {(r['case'],r['repeat'])for r in warm}=={(c,j)for c in expected for j in range(3)}
 for r in warm:assert r['operations']==1000 and r['allocations']==r['deallocations']==10000 and r['requested_bytes']==r['deallocated_bytes']==582000
print('PASS: 3 processes; 27 warm count records /27000 operations; 3 cold TLS records; source restored')
