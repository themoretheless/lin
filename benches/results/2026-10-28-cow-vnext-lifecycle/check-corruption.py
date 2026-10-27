from pathlib import Path
import json,shutil,tempfile,subprocess
src=Path(__file__).resolve().parent;results=[]
for case in ['ledger','readback','missing-process']:
 with tempfile.TemporaryDirectory(prefix='lin-cow-ledger-negative-') as temp:
  p=Path(temp)
  for f in src.iterdir():
   if f.is_file() and f.name!='summary.json':shutil.copy2(f,p/f.name)
  f=p/'candidate-1000-1.json';d=json.loads(f.read_text())
  if case=='ledger':d['held']['live']+=1;f.write_text(json.dumps(d))
  elif case=='readback':d['exact_all_rows_readback']=False;f.write_text(json.dumps(d))
  else:
   f=p/'status.json';d=json.loads(f.read_text());f.write_text(json.dumps(d[:-1]))
  r=subprocess.run(['python3',str(p/'analyze.py')],capture_output=True,text=True);assert r.returncode!=0 and not (p/'summary.json').exists(),case;results.append(case)
(src/'corruption-status.json').write_text(json.dumps(dict(rejected=results,actual_raw_modified=False,latency_qualified=False,rss_qualified=False),indent=2)+'\n');print('PASS:',','.join(results))
