from pathlib import Path
import json,hashlib,re
from collections import Counter
p=Path(__file__).resolve().parent;records=json.loads((p/'codegen.json').read_text());result=[]
for record in records:
 calls=[];bodies=[];targets=[]
 for file in record['files']:
  f=p/file['saved_path'];assert hashlib.sha256(f.read_bytes()).hexdigest()==file['sha256'];current=None;body=None;recent=[];target=[]
  with f.open()as stream:
   for line in stream:
    if line.startswith('target '):target.append(line.rstrip())
    if line.startswith('define '):
     current=line.rstrip();body=[]if'index_covers_pred'in line else None
    if body is not None:body.append(line)
    if current and 'index_covers_pred'in line and re.search(r'\b(?:call|invoke)\b',line):calls.append(dict(caller=current,caller_symbol=re.search(r'@([^ (]+)',current).group(1),context=recent[-3:]+[line.rstrip()]))
    if line.rstrip()=='}':
     if body is not None:bodies.append(''.join(body))
     current=None;body=None
    recent.append(line.rstrip());recent=recent[-3:]
  targets.extend(target)
 assert len(bodies)==1 and len(calls)==18
 (p/(record['role']+'-coverage.ll')).write_text(bodies[0])
 result.append(dict(role=record['role'],index_sha256=record['index_sha256'],definition_count=len(bodies),direct_call_count=len(calls),coverage_body_lines=len(bodies[0].splitlines()),coverage_body_sha256=hashlib.sha256(bodies[0].encode()).hexdigest(),target=targets,calls=calls,performance_qualified=False))
assert result[0]['target']==result[1]['target'];same_callers=Counter(x['caller_symbol']for x in result[0]['calls'])==Counter(x['caller_symbol']for x in result[1]['calls'])
(p/'summary.json').write_text(json.dumps(dict(diagnostic_complete=True,roles=result,same_direct_caller_symbols=same_callers,performance_qualified=False,full_objective_achieved=False,scope='local optimized library IR; does not rule out partial inlining or explain historical Linux regression'),indent=2)+'\n')
print([(r['role'],r['definition_count'],r['direct_call_count'],r['coverage_body_lines'])for r in result], 'same caller symbols:',same_callers)
