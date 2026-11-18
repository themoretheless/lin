import json
from pathlib import Path
import statistics
import subprocess
import sys

worker, output = map(Path, sys.argv[1:])
request = json.dumps(dict(insert_rows=100_000, samples=1, timestamp_ms=123456, column_batch=True))
samples = []
for i in range(13):
    r = subprocess.run([worker.resolve()], input=request + "\n", text=True, capture_output=True, check=True)
    data = json.loads(r.stdout)
    assert data["api"] == "column batch" and len(data["result"]) == 100_000
    stages = {}
    for line in r.stderr.splitlines():
        end = line.split()
        if end[0] == "BATCH_MATERIALIZE":
            stages.update(zip(("typecheck", "rows"), map(int, end[1:])))
        elif end[0] == "BATCH_MAINTAIN":
            stages.update(zip(("keys", "embed", "maintain"), map(int, end[1:])))
    assert len(stages) == 5 and sum(stages.values()) <= data["samples_ns"][0]
    samples.append(dict(total=data["samples_ns"][0], **stages))
    output.write_text(json.dumps(samples, indent=2) + "\n")
for name in samples[0]:
    print(name, statistics.median(s[name] for s in samples[1:]) / 1e6, "ms")
