from pathlib import Path
import hashlib,json,re
p=Path(__file__).resolve().parent;load=lambda n:json.loads((p/n).read_text());m=load('test-status.json');mut=load('mutant-status.json');h=lambda f:hashlib.sha256(Path(f).read_bytes()).hexdigest()
assert m['exit_code']==0 and m['full_suite']and m['source_restored']and not m['performance_qualified']
assert all(h(f)==digest for f,digest in m['source_sha256'].items())and h(p/'helper.rs')==m['helper_sha256']and h(p/'oracle.rs')==m['oracle_sha256']
text=(p/'full-tests.log').read_text();counts=[tuple(map(int,r))for r in re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',text)];assert[sum(r[i]for r in counts)for i in range(3)]==[436,0,2]
assert 'canonical_frame_budget_is_not_relaxed_by_zero_compaction ... ok'in text and 'legacy_decoder_preserves_complete_records_and_ieee_bits ... ok'in text
wire=[json.JSONDecoder().raw_decode(part)[0]for part in text.split('ZERO_WIRE_PROBE ')[1:]];assert len(wire)==2 and all(r==dict(canonical_frame=4125,canonical_payload=4112,codec=1,compact_frame=1169,compact_payload=1156,positive_zeros=739)for r in wire)
assert mut['exit_code']!=0 and mut['expected_failure']and mut['runtime_assertion_caught']and 'test result: FAILED'in(p/'mutant.log').read_text()
print('Functional audit PASS:436 release tests /2 ignored;legacy decoder and canonical16MiB budget preserved;4125->1169 byte frames;negative-zero mutant caught;no latency acceptance')
