from pathlib import Path
import subprocess,json,os,time
p=Path(__file__).resolve().parent
status={}
resources=[]
for series in [1,2]:
 for mode in ["native","durable"]:
  for pair in range(1,7):
   for v in (["baseline","candidate"] if pair%2 else ["candidate","baseline"]):
    name=f"rust-s{series}-{mode}-{v}-{pair}"
    env=dict(os.environ,LIN_BENCH_SKIP_MYSQL="1");env.pop("LIN_BENCH_MIXED_CASE",None)
    cmd=["/tmp/lin-nochange-"+v,"--filter","compare/insert_bulk_*/*" if mode=="native" else "compare/durable_insert_*/*","--glob","--exclude","*/duckdb","--samples","24","--max-iterations","1","--warmup-ms","0","--sample-ms","1","--output",str(p/name)]
    before=os.getloadavg();start=time.monotonic()
    with (p/(name+".log")).open("w") as log:r=subprocess.run(cmd,env=env,stdout=log,stderr=subprocess.STDOUT)
    resources.append({"run":name,"load_before":before,"load_after":os.getloadavg(),"wall_seconds":time.monotonic()-start});(p/"host-load.json").write_text(json.dumps(resources,indent=2))
    status[name]=r.returncode;(p/"rust-status.json").write_text(json.dumps(status,indent=2));print(name,r.returncode,flush=True)
    if r.returncode:raise SystemExit(r.returncode)
