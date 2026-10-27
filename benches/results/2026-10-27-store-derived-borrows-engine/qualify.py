from pathlib import Path
import json,sys
p=Path(__file__).resolve().parent;q=json.loads((p/'qualification.json').read_text());assert q['complete_rounds']==12 and q['processes']==1332 and q['observations']==10656 and q['full_objective_achieved']is False
print(json.dumps(q,indent=2));sys.exit(0 if q['source_candidate_qualified']else 3)
