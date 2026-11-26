"""Collect phase timings on the same runner as the paired DuckDB comparison."""
import json
import os
from pathlib import Path
from statistics import median
import subprocess
import sys

worker, output = (Path(arg).resolve() for arg in sys.argv[1:])
phases = dict(BEGIN=1, BUILD=10, CHECK=10, MAINTAIN=10, CHUNK=10, COMMIT=1)
request = json.dumps(dict(insert_rows=100_000, samples=1, timestamp_ms=123456))
samples = []
for _ in range(12):
    run = subprocess.run([worker], input=request + "\n", text=True, capture_output=True,
                         env=dict(os.environ, LIN_BENCH_INSERT_PHASES="1"), check=True)
    result = json.loads(run.stdout)
    assert result["statements"] == 10 and len(result["result"]) == 100_000
    sample = dict(total_ns=result["samples_ns"][0])
    for phase, count in phases.items():
        prefix = f"INSERT_{phase} "
        values = [int(line.removeprefix(prefix)) for line in run.stderr.splitlines()
                  if line.startswith(prefix)]
        assert len(values) == count and all(ns > 0 for ns in values), (phase, values)
        sample[phase] = values
    samples.append(sample)
    output.write_text(json.dumps(samples, indent=2) + "\n")
print("100k phase medians (ms)", {
    key: round(median(x["total_ns"] if key == "total_ns" else sum(x[key])
                      for x in samples) / 1e6, 3)
    for key in ("total_ns", *phases)
})
