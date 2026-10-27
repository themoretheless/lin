from pathlib import Path
import subprocess,json
p=Path(__file__).resolve().parent
subprocess.run(['python3',str(p/'check-negative-zero-mutant.py')],check=True)
m=json.loads((p/'negative-zero-mutant-status.json').read_text());assert m['runtime_mutant_caught'] and m['source_restored']
(p/'mutant-status.json').write_text(json.dumps(dict(exit_code=m['exit_code'],expected_failure=True,runtime_assertion_caught=True),indent=2)+'\n')
