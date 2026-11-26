"""Measure the two builds on alternating fresh processes with default glibc."""
import json
import os
import subprocess
import sys

env = dict(os.environ, LIN_BENCH_INSERT_RUSAGE="1")
for key in ("LD_PRELOAD", "MALLOC_CONF", "GLIBC_TUNABLES"):
    env.pop(key, None)
results = []
for rows in (10000, 100000):
    request = json.dumps(dict(insert_rows=rows, samples=3, timestamp_ms=123456)) + "\n"
    programs = list(zip(("base", "candidate"), sys.argv[1:], strict=True))
    for trial in range(8):
        for label, program in programs:
            data = json.loads(subprocess.check_output([program], input=request, text=True, env=env))
            assert len(data["result"]) == rows
            assert len(data["insert_usage"]) == len(data["samples_ns"]) == 3
            results.append(dict(rows=rows, trial=trial, build=label,
                                timings_ns=data["samples_ns"], resources=data["insert_usage"]))
        programs.reverse()
print(json.dumps(results, indent=2))
