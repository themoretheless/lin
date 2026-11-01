from pathlib import Path
import json
p=Path(__file__).resolve().parent
q=json.loads((p/'benchmark-measurement/qualification.json').read_text());a=json.loads((p/'benchmark-independent-audit.json').read_text())
assert a['complete']and q['measurement_complete']and q['component_gate_passes']and a['component_gate_passes'], 'Frozen component performance criteria failed; keep all raw results'
assert not q['full_objective_achieved']
