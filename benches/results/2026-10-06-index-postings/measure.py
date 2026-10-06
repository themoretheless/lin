"""Alternate fresh baseline/final test processes; compare local component timings."""
import hashlib, json, pathlib, re, statistics, subprocess, sys
out=pathlib.Path(__file__).resolve().parent
bins=dict(zip(["baseline","candidate"],sys.argv[1:3]))
assert len(bins)==2
index_only = "--index-only" in sys.argv[3:]
runs=[]
for pair in range(7):
 for variant in (["baseline","candidate"] if pair%2==0 else ["candidate","baseline"]):
  values={};outputs=[]
  for test in (["index::storage_tests::index_optimization_measurement"] if index_only else ["index::storage_tests::index_optimization_measurement","fts::tests::posting_optimization_measurement"]):
   p=subprocess.run([bins[variant],test,"--exact","--ignored","--nocapture"],capture_output=True,text=True,check=True)
   outputs.append(p.stdout)
   values.update({name:int(ns) for name,ns in re.findall(r"^((?:unchanged|unique|postings) .*): (\d+) ns$",p.stdout,re.M)})
  assert len(values)==(4 if index_only else 10)
  runs.append(dict(pair=pair,variant=variant,values=values,outputs=outputs))
medians={v:{k:statistics.median(r["values"][k] for r in runs if r["variant"]==v) for k in runs[0]["values"]} for v in bins}
result=dict(binary_sha256={v:hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest() for v,p in bins.items()},runs=runs,medians_ns=medians,speedup={k:medians["baseline"][k]/medians["candidate"][k] for k in medians["baseline"]})
(out/"run.json").write_text(json.dumps(result,indent=2));print(json.dumps({"medians":medians,"speedup":result["speedup"]},indent=2))
