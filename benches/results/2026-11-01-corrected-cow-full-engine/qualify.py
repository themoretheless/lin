from pathlib import Path
import json
p=Path(__file__).resolve().parent;q=json.loads((p/'qualification.json').read_text())
assert q['measurement_complete']and q['source_candidate_qualified'],'Full source gate failed; preserve all observations and controls'
