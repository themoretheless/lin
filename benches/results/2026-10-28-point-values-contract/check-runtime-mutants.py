from pathlib import Path
import sys, subprocess, os, json
sys.path.insert(0,"scripts")
from cargo_bench_artifact import bench_executable
p=Path("benches/point_values.rs"); original=p.read_text(); root=Path("benches/results/2026-10-28-point-values-contract")
results=[]
try:
 for engine, fn, old, new in [("lin","lin_rows",'row.get("title")','row.get("id")'),("sqlite","sqlite_rows",'row.get::<_, String>(1)','row.get::<_, String>(0)'),("duckdb","duck_rows",'row.get::<_, String>(1)','row.get::<_, String>(0)')]:
  start=original.index("fn "+fn+"("); end=original.index("\n}",start)+2
  body=original[start:end]; assert body.count(old)==1
  mutant=original[:start]+body.replace(old,new)+original[end:]
  p.write_text(mutant); (root/(engine+"-mutant.rs")).write_text(mutant)
  env=dict(os.environ,CARGO_TARGET_DIR="/tmp/lin-cow-vnext-target")
  b=subprocess.run(["cargo","bench","--bench","point_values","--no-run"],env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
  (root/(engine+"-mutant-build.log")).write_text(b.stdout); assert b.returncode==0,b.stdout[-2000:]
  binary=bench_executable(b.stdout,"point_values")
  r=subprocess.run([binary,"--filter","point_values/hit/"+engine,"--exact","--samples","1","--max-iterations","1","--warmup-ms","0","--sample-ms","1","--output",str(root/(engine+"-mutant"))],stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
  (root/(engine+"-mutant-runtime.log")).write_text(r.stdout)
  files=list((root/(engine+"-mutant")).rglob("*.json"))
  texts=r.stdout+"".join(x.read_text() for x in files)
  caught="id/title bytes or multiplicity mismatch" in texts
  assert caught, texts[-3000:]
  results.append(dict(engine=engine,build_exit=b.returncode,runtime_exit=r.returncode,wrong_title_caught=caught))
finally:
 p.write_text(original)
(root/"mutant-status.json").write_text(json.dumps(dict(checks=results,source_restored=p.read_text()==original,performance_qualified=False),indent=2)+"\n")
print(json.dumps(results))
