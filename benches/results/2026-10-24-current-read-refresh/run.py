from pathlib import Path
import subprocess,json,sys
p=Path(__file__).resolve().parent
statuses={}
for n in [1000,10000,100000]:
 with (p/f"rows-{n}.log").open("w") as log:
  r=subprocess.run([".bench-venv/bin/python","scripts/bench-peers.py","--engines","lin","sqlite","duckdb","pandas","--rows",str(n),"--samples","12","--repeats","3","--lin-binary","/tmp/lin-read-refresh-28af2dc","--require-wins","--output",str(p/f"rows-{n}")],stdout=log,stderr=subprocess.STDOUT)
 statuses[str(n)]=r.returncode
 (p/"run-status.json").write_text(json.dumps(statuses,indent=2))
 print(n,r.returncode,flush=True)
 if r.returncode not in [0,3]:break
