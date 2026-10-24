from pathlib import Path
import subprocess,json
p=Path(__file__).resolve().parent
status={}
for series in [1,2]:
 for pair in range(1,7):
  for v in (["baseline","candidate"] if pair%2 else ["candidate","baseline"]):
   binary="/tmp/lin-read-refresh-28af2dc" if v=="baseline" else "/tmp/lin-prepared-count-candidate"
   for n in [1000,10000,100000]:
    name=f"s{series}-p{pair}-{v}-{n}"
    cmd=[".bench-venv/bin/python","scripts/bench-peers.py","--engines","lin","sqlite","duckdb","pandas","--cases","filter_eq","text_substr","--rows",str(n),"--samples","12","--repeats","1","--lin-binary",binary,"--require-wins","--output",str(p/name)]
    with (p/(name+".log")).open("w") as log:r=subprocess.run(cmd,stdout=log,stderr=subprocess.STDOUT)
    status[name]=r.returncode
    (p/"pairs-status.json").write_text(json.dumps(status,indent=2))
    print(name,r.returncode,flush=True)
    if r.returncode not in [0,3]:raise SystemExit(r.returncode)
