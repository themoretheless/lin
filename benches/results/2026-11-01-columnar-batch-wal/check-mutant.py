from pathlib import Path
import subprocess,json,hashlib,os
p=Path(__file__).resolve().parent;source=Path('src/persist.rs');before=source.read_bytes();h=lambda b:hashlib.sha256(b).hexdigest();status=json.loads((p/'test-status.json').read_text());assert status['passed']==460 and status['persist_sha256']==h(before)
needle=b'for pack in &packs[1..] {';assert before.count(needle)==1;mutant=before.replace(needle,b'for pack in &packs[1..1] {');result=dict(source_before_sha256=h(before),mutant_sha256=h(mutant),runtime_assertion_caught=False)
try:
 source.write_bytes(mutant);(p/'lost-child-mutant.rs').write_bytes(mutant)
 with(p/'lost-child-runtime.log').open('w')as log:r=subprocess.run(['cargo','test','--release','--lib','column_batch_preserves_order_bits_and_existing_wire_replay','--color','never'],stdout=log,stderr=subprocess.STDOUT)
 text=(p/'lost-child-runtime.log').read_text();result.update(exit_code=r.returncode,runtime_assertion_caught=r.returncode==101 and 'column_batch_preserves_order_bits_and_existing_wire_replay ... FAILED'in text and 'test result: FAILED'in text and 'error[E'not in text)
finally:
 source.write_bytes(before);result['source_restored']=source.read_bytes()==before;(p/'mutant-status.json').write_text(json.dumps(result,indent=2)+'\n')
assert result['source_restored']and result['runtime_assertion_caught'],result
