"""Split the Lin side of the DuckDB INSERT 100k case into transaction phases."""
import json
from pathlib import Path
import subprocess
import sys
from statistics import median

worker, output = map(Path, sys.argv[1:])
request = json.dumps(dict(insert_rows=100_000, samples=1, timestamp_ms=123456, profile_insert=True))
samples = []
for _ in range(13):
    run = subprocess.run([worker.resolve()], input=request + "\n", text=True, capture_output=True, check=True)
    result = json.loads(run.stdout)
    assert len(result["result"]) == 100_000
    sample = result["phase_samples"][0]
    sample["total_ns"] = result["samples_ns"][0]
    phases = [sample["begin_ns"], *sample["chunks_ns"], sample["commit_ns"]]
    assert len(phases) == 12 and min(phases) > 0
    assert sum(phases) <= sample["total_ns"]
    for name in ("BUILD", "CHECK", "MAINTAIN", "HASH_CHUNK"):
        sample[name] = [line.split()[1:] for line in run.stderr.splitlines()
                        if line.startswith(f"INSERT_{name} ")]
    assert all(len(sample[name]) == 10 for name in ("BUILD", "CHECK", "MAINTAIN"))
    assert sum(int(x[0]) for x in sample["HASH_CHUNK"]) == 100_000
    samples.append(sample)
    output.write_text(json.dumps(samples, indent=2) + "\n")
measured = samples[1:]
for phase in ("begin_ns", "commit_ns", "total_ns"):
    print(phase, median(x[phase] for x in measured) / 1e6, "ms")
print("chunks_ms", [median(x["chunks_ns"][i] for x in measured) / 1e6 for i in range(10)])

for name in ("BUILD", "CHECK", "MAINTAIN"):
    print(name, median(sum(int(p[0]) for p in x[name]) for x in measured) / 1e6, "ms")
print("Hashing aggregate worker time",
      median(sum(int(p[1]) for p in x["HASH_CHUNK"]) for x in measured) / 1e6, "ms")
