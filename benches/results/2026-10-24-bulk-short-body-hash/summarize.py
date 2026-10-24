from pathlib import Path
import json,statistics
p=Path(__file__).resolve().parent
out={}
for s in [1,2]:
 for n in [1000,10000,100000]:
  for e in ["lin","sqlite","duckdb"]:
   vals={}
   for v in ["baseline","candidate"]:
    vals[v]=[]
    for pair in range(1,7):
     d=json.loads((p/f"s{s}-p{pair}-{v}-{n}"/"run.json").read_text());assert d["status"]=="complete" and not d["errors"]
     vals[v].append(d["medians_ns"]["insert_native/"+e])
   a,b=vals["baseline"],vals["candidate"]
   out[f"s{s}/{n}/{e}"]={**vals,"baseline_median_ns":statistics.median(a),"candidate_median_ns":statistics.median(b),"paired_gain_percent":statistics.median((x-y)/x*100 for x,y in zip(a,b)),"wins":sum(y<x for x,y in zip(a,b))}
(p/"medians.json").write_text(json.dumps(out,indent=2))
for k,v in out.items():print(k,round(v["paired_gain_percent"],3),v["wins"],round(v["candidate_median_ns"]/1e6,3))
