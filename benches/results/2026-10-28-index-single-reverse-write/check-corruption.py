from pathlib import Path
import json,tempfile,shutil,subprocess
p=Path(__file__).resolve().parent;checks=[]
for mutation in ['missing-process','negative-wall','changed-summary']:
 with tempfile.TemporaryDirectory(prefix='lin-index-audit-')as td:
  target=Path(td)/'raw';shutil.copytree(p,target,ignore=shutil.ignore_patterns('raw-artifact.tar.gz','raw-manifest.json'))
  if mutation=='missing-process':
   f=target/'process-status.json';rows=json.loads(f.read_text());f.write_text(json.dumps(rows[:-1]))
  elif mutation=='negative-wall':
   f=target/'01-baseline.jsonl';rows=[json.loads(line)for line in f.read_text().splitlines()];rows[0]['ns_per_update']=-1;f.write_text('\n'.join(json.dumps(x)for x in rows)+'\n')
  else:
   f=target/'summary.json';s=json.loads(f.read_text());case=next(iter(s));s[case]['series1']['passes']=not s[case]['series1']['passes'];f.write_text(json.dumps(s))
  r=subprocess.run(['python3',str(p/'audit.py'),str(target)],capture_output=True,text=True);assert r.returncode!=0,mutation;checks.append(dict(mutation=mutation,rejected=True))
(p/'audit-sensitivity.json').write_text(json.dumps(dict(actual_raw_unchanged=True,checks=checks),indent=2)+'\n');print('All three corruptions rejected')
