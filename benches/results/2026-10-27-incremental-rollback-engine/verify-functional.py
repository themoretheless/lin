from pathlib import Path
import hashlib,json,re
p=Path(__file__).resolve().parent;m=json.loads((p/'test-status.json').read_text());mut=json.loads((p/'mutant-status.json').read_text());h=lambda f:hashlib.sha256(Path(f).read_bytes()).hexdigest();assert m['exit_code']==0 and m['source_restored'] and not m['performance_qualified']
assert all(h(f)==v for f,v in m['source_sha256'].items());text=(p/'full-tests.log').read_text();counts=re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',text);assert [sum(int(r[i])for r in counts)for i in range(3)]==[434,0,2]
for name in ['owned_rollback_restores_changed_identity_and_row_maps','owned_rollback_reverses_repeated_updates','incremental_rollback_preserves_fts_after_success_and_index_failure']:assert name+' ... ok'in text
assert mut['exit_code']!=0 and mut['expected_failure']and mut['runtime_assertion_caught']and 'test result: FAILED'in(p/'mutant.log').read_text();print('Functional audit PASS:434 release tests/2 ignored, identity/repeated update/index failure/FTS recovery and runtime missing-FTS mutant')

warm=json.loads((p/'warm-summary.json').read_text());wm=json.loads((p/'warm-provenance.json').read_text());assert warm['diagnostic_gate_passes'] and wm['source_sha256']==m['source_sha256']
for source,digest in wm['binaries']['candidate']['candidate_sources_sha256'].items():assert h(p/('candidate-'+Path(source).name))==digest
