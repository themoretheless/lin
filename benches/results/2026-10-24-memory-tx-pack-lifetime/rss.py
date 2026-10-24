from pathlib import Path
import subprocess,json,re
p=Path(__file__).resolve().parent
observations=[]
for pair in range(1,4):
 for v in (["baseline","candidate"] if pair%2 else ["candidate","baseline"]):
  binary="/tmp/lin-bounded-native-insert" if v=="baseline" else "/tmp/lin-memory-tx-pack-candidate"
  name=f"rss-{pair}-{v}"
  with (p/(name+".log")).open("w") as log:
   r=subprocess.run(["/usr/bin/time","-l",binary],input=json.dumps({"insert_rows":100000,"samples":12,"timestamp_ms":123456})+"\n",stdout=subprocess.PIPE,stderr=log,text=True,check=True)
  d=json.loads(r.stdout);assert len(d["result"])==100000 and d["result"][-1][0]=="d-99999" and d["result"][-1][4]==123456
  rss=int(re.search(r"(\d+)\s+maximum resident set size",(p/(name+".log")).read_text())[1])
  observations.append({"pair":pair,"variant":v,"max_rss_bytes":rss,"sample_count":len(d["samples_ns"]),"validated_rows":len(d["result"])})
  print(name,rss,flush=True)
(p/"rss.json").write_text(json.dumps({"scope":"whole worker process peak, including parsing/store/canonicalization/output; not isolated insertion-phase live memory","observations":observations},indent=2))
