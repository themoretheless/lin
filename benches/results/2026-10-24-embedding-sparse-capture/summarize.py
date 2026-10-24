from pathlib import Path
import json,statistics
p=Path(__file__).resolve().parent
out={}
for series in [1,2]:
 for mode in ["native","durable"]:
  seed=json.loads((p/f"rust-s{series}-{mode}-baseline-1/run.json").read_text())
  for case in seed["cases"]:
   key=case["id"]
   vals={}
   for v in ["baseline","candidate"]:
    vals[v]=[]
    for i in range(1,7):
     d=json.loads((p/f"rust-s{series}-{mode}-{v}-{i}/run.json").read_text());assert d["status"]=="complete"
     obs=[x for x in d["observations"] if x["case"]==key];assert len(obs)==24
     vals[v].append(statistics.median(float(x["value"])/x["operations"] for x in obs))
   a,b=vals.values();out[f"s{series}/{key}"]={**vals,"baseline_median_ns":statistics.median(a),"candidate_median_ns":statistics.median(b),"paired_gain_percent":statistics.median((x-y)/x*100 for x,y in zip(a,b)),"wins":sum(y<x for x,y in zip(a,b))}
(p/"rust-medians.json").write_text(json.dumps(out,indent=2))
for k,v in out.items():print(k,round(v["paired_gain_percent"],3),v["wins"],round(v["candidate_median_ns"]/1e6,3))
