from pathlib import Path
import subprocess,json,hashlib
p=Path(__file__).resolve().parent;source=Path('src/fts.rs');original=source.read_bytes();text=original.decode();marker='    out.sort_unstable();\n    out.dedup();\n    out\n}'
assert text.count(marker)==1
mutant=text.replace(marker,'    out.truncate(8);\n'+marker)
(p/'truncated-spill-mutant.rs').write_text(mutant)
try:
 source.write_text(mutant)
 with (p/'truncated-spill-runtime.log').open('w')as log:r=subprocess.run(['cargo','test','--release','--lib','inline_tokens_spill_preserves_terms_and_posting_edits','--color','never'],stdout=log,stderr=subprocess.STDOUT)
 log=(p/'truncated-spill-runtime.log').read_text();assert r.returncode==101 and 'test result: FAILED.' in log and 'inline_tokens_spill_preserves_terms_and_posting_edits ... FAILED' in log,log[-3000:]
finally:
 source.write_bytes(original)
 (p/'mutant-status.json').write_text(json.dumps(dict(source_restored=source.read_bytes()==original,original_sha256=hashlib.sha256(original).hexdigest(),mutant_sha256=hashlib.sha256(mutant.encode()).hexdigest(),exit_code=r.returncode if 'r'in locals()else None,runtime_assertion_caught=('test result: FAILED.'in log)if isinstance(locals().get('log'),str)else False),indent=2)+'\n')
print('Compiled truncation mutant rejected at runtime; source restored')
