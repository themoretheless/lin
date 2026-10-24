from pathlib import Path
import json,statistics
p=Path(__file__).resolve().parent
out={}
for s in [1,2]:
 for n in [1000,10000,100000]:
  for case in ["filter_eq","text_substr"]:
   for e in ["lin","sqlite","duckdb","pandas"]:
    vals={}
    for v in ["baseline","candidate"]:
     vals[v]=[]
     for pair in range(1,7):
      f=p/f"s{s}-p{pair}-{v}-{n}"/"run.json"
      d=json.loads(f.read_text());assert d["status"]=="complete" and not d["errors"]
      vals[v].append(d["medians_ns"][case+"/"+e])
    a,b=vals["baseline"],vals["candidate"]
    out[f"s{s}/{n}/{case}/{e}"]={**vals,"baseline_median_ns":statistics.median(a),"candidate_median_ns":statistics.median(b),"paired_gain_percent":statistics.median((x-y)/x*100 for x,y in zip(a,b)),"wins":sum(y<x for x,y in zip(a,b))}
(p/"medians.json").write_text(json.dumps(out,indent=2))
for k,v in out.items():
 if k.endswith("/lin"):print(k,round(v["paired_gain_percent"],3),v["wins"],round(v["candidate_median_ns"]/1000,3))
