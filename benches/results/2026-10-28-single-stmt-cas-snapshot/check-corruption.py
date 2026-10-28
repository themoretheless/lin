from pathlib import Path
import tempfile,shutil,json,subprocess
p=Path(__file__).resolve().parent
results=[]
for mutation in ['missing-process','negative-wall','changed-summary']:
 with tempfile.TemporaryDirectory(prefix='lin-small-row-audit-') as td:
  target=Path(td)/'copy';shutil.copytree(p,target,ignore=shutil.ignore_patterns('raw-artifact.tar.gz','raw-manifest.json'))
  if mutation=='missing-process':
   f=target/'process-status.json';rows=json.loads(f.read_text());f.write_text(json.dumps(rows[:-1]))
  elif mutation=='negative-wall':
   record=json.loads((target/'process-status.json').read_text())[0]
   f=target/f"{record['round']:02}-{record['mode']}"/record['case'].replace('/','_')/'run.json';data=json.loads(f.read_text());data['observations'][0]['value']='-1';f.write_text(json.dumps(data))
  else:
   f=target/'summary.json';data=json.loads(f.read_text());case=next(iter(data));data[case]['series1']['passes']=not data[case]['series1']['passes'];f.write_text(json.dumps(data))
  result=subprocess.run(['python3',str(p/'audit.py'),str(target)],capture_output=True,text=True);assert result.returncode!=0,mutation;results.append(dict(mutation=mutation,rejected=True))
(p/'audit-sensitivity.json').write_text(json.dumps(dict(actual_raw_unchanged=True,checks=results),indent=2)+'\n');print('All three corruptions rejected')
