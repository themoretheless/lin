from pathlib import Path
import json,re
p=Path(__file__).resolve().parent;root=p.parents[2];workflow=root/'.github/workflows/cow-extent-public-peers.yml';prefix=str(p.relative_to(root))
def validate(text):
 refs=re.findall(r'benches/results/[A-Za-z0-9_./-]+',text);assert refs
 assert all(ref.rstrip('/')==prefix or ref.startswith(prefix+'/')for ref in refs),'Workflow references a different protocol directory'
 for ref in refs:
  path=root/ref
  if ref.endswith('.py')or ref.endswith('/input.json'):assert path.is_file(),ref
  elif ref.rstrip('/')!=prefix:assert path.parent.is_dir(),ref
 return len(refs)
text=workflow.read_text();count=validate(text)
try:validate(text.replace(prefix,prefix.replace('cow-extent','corrected-cow-extent'),1))
except AssertionError:pass
else:raise AssertionError('Wrong protocol prefix not rejected')
try:validate(text.replace('/check-host.py','/missing-host-check.py'))
except AssertionError:pass
else:raise AssertionError('Missing script not rejected')
(p/'workflow-path-audit.json').write_text(json.dumps(dict(validated_references=count,wrong_prefix_rejected=True,missing_script_rejected=True,measurement_processes=0),indent=2)+'\n');print('PASS workflow protocol references; wrong prefix and missing script rejected')
