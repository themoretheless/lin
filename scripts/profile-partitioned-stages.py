"""Collect per-batch times on a fresh worker for each three-insert trial."""
import json
import subprocess
import sys

request = json.dumps(dict(insert_rows=100_000, samples=3, timestamp_ms=123456))
trials = []
for _ in range(8):
    run = subprocess.run([sys.argv[1]], input=request, text=True, capture_output=True, check=True)
    result = json.loads(run.stdout)
    stages = {"build": [], "reserve": []}
    for line in run.stderr.splitlines():
        if line.startswith("insert_profile "):
            _, stage, duration = line.split()
            stages[stage].append(int(duration))
    if any(len(values) != 30 for values in stages.values()):
        raise ValueError(f"Expected ten measured batches per insert: {[(k,len(v)) for k,v in stages.items()]}")
    if result["statements"] != 10 or len(result["result"]) != 100_000:
        raise ValueError("Incomplete insert")
    trials.append(dict(total_ns=result["samples_ns"], components_ns=stages))
print(json.dumps(trials))
