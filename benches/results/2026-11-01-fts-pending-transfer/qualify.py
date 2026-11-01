from pathlib import Path
import json
p=Path(__file__).resolve().parent
q=json.loads((p/'measurement/qualification.json').read_text());assert q['measurement_complete']and q['component_gate_passes'],'Component gate failed; preserve all observations'
