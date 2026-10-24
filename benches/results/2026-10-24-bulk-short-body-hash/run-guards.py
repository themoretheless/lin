from pathlib import Path
import subprocess,json,os,statistics,hashlib
p=Path(__file__).resolve().parent
out=[]
(p/"guard-binary-sha256.json").write_text(json.dumps({v:hashlib.sha256(Path("/tmp/lin-bulk-hash-guard-"+v).read_bytes()).hexdigest() for v in ["baseline","candidate"]},indent=2))
for pair in range(1,7):
 for v in (["baseline","candidate"] if pair%2 else ["candidate","baseline"]):
  for mode in ["short_repeat","long_repeat"]:
   for n in [1000,10000]:
    name=f"guard-p{pair}-{v}-{mode}-{n}"
    with (p/(name+".log")).open("w") as log:r=subprocess.run(["/tmp/lin-bulk-hash-guard-"+v],input=json.dumps({"insert_rows":n,"samples":12,"timestamp_ms":123456})+"\n",stdout=subprocess.PIPE,stderr=log,env=dict(os.environ,LIN_HASH_BODY_MODE=mode),text=True,check=True)
    d=json.loads(r.stdout);rows=d["result"];assert len(rows)==n
    for i,row in enumerate(rows):
     title=f"doc {i} wal note" if i%10==0 else f"doc {i} plain"
     assert row==[f"d-{i}",f"bench://{i}","rag" if i%2==0 else "sys",title,123456,"same body" if mode=="short_repeat" else "x"*256]
    out.append({"pair":pair,"variant":v,"mode":mode,"rows":n,"samples_ns":d["samples_ns"],"median_ns":statistics.median(d["samples_ns"]),"validated_rows":n})
    (p/"guard-observations.json").write_text(json.dumps(out,indent=2))
  print(pair,v,flush=True)
summary={}
for mode in ["short_repeat","long_repeat"]:
 for n in [1000,10000]:
  vals={v:[x["median_ns"] for x in out if x["variant"]==v and x["mode"]==mode and x["rows"]==n] for v in ["baseline","candidate"]}
  a,b=vals.values();summary[f"{mode}/{n}"]={**vals,"paired_gain_percent":statistics.median((x-y)/x*100 for x,y in zip(a,b)),"wins":sum(y<x for x,y in zip(a,b))}
(p/"guard-summary.json").write_text(json.dumps(summary,indent=2));print(json.dumps(summary),flush=True)
