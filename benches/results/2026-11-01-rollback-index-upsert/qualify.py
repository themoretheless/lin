from pathlib import Path
import json
q=json.loads((Path(__file__).resolve().parent/'qualification.json').read_text())
assert q['measurement_complete']and q['component_gate_passes'],'Component gate failed; preserve every observation'
assert not q['full_engine_qualified']and not q['full_objective_achieved']
