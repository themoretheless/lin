from pathlib import Path
import subprocess,json,re,hashlib,importlib.util,statistics
p=Path(__file__).resolve().parent
assert Path("src/exec.rs").read_bytes()==(p/"before-exec.rs").read_bytes()
meta={"source_commit":subprocess.check_output(["git","rev-parse","HEAD"],text=True).strip(),"source_sha256":{str(f):hashlib.sha256(f.read_bytes()).hexdigest() for f in sorted(Path("src").rglob("*.rs"))},"instrumented_exec_sha256":hashlib.sha256((p/"instrumented-exec.rs").read_bytes()).hexdigest()}
(p/"provenance.json").write_text(json.dumps(meta,indent=2))
spec=importlib.util.spec_from_file_location("bench",Path("scripts/bench-peers.py"));bench=importlib.util.module_from_spec(spec);spec.loader.exec_module(bench)
want=[[d[0],d[1],d[2],d[3],123456,d[4]] for d in bench.dataset(100000)[0]]
events=[];runs=[]
for i in range(1,4):
 with (p/f"process-{i}.log").open("w") as log:r=subprocess.run(["/tmp/lin-chunked-100k-profile"],input=json.dumps({"insert_rows":100000,"samples":12,"timestamp_ms":123456})+"\n",stdout=subprocess.PIPE,stderr=log,text=True,check=True)
 d=json.loads(r.stdout);bench.validate(d["result"],want,"lin","100k-profile");assert d["statements"]==10
 parsed=[{k:int(v) for k,v in re.findall(r"(\w+)=(\d+)",line)} for line in (p/f"process-{i}.log").read_text().splitlines() if line.startswith("INSERT_PHASE ")]
 assert len(parsed)==120 and all(x["rows"]==10000 for x in parsed)
 assert [x["start"] for x in parsed]==list(range(0,100000,10000))*12
 events.extend({"process":i,"sample":j//10,"chunk":j%10,**e} for j,e in enumerate(parsed));runs.append({"process":i,"validated_rows":len(d["result"]),"samples_ns":d["samples_ns"]});print(i,"validated",flush=True)
(p/"events.json").write_text(json.dumps(events,indent=2));(p/"runs.json").write_text(json.dumps(runs,indent=2))
keys=["build_ns","checks_ns","embed_ns","index_ns","fts_ns","hnsw_ns","maps_ns","total_before_store_ns"]
summary={}
for chunk in range(10):
 summary[str(chunk)]={k:statistics.median(e[k] for e in events if e["chunk"]==chunk) for k in keys}
(p/"phase-medians.json").write_text(json.dumps(summary,indent=2))
assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in meta["source_sha256"].items())
print(json.dumps(summary),flush=True)
