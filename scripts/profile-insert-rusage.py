"""Compare Linux process resource use during first and subsequent INSERTs."""
import json
import os
from pathlib import Path
from statistics import median
import subprocess
import sys

worker, output = map(Path, sys.argv[1:])
base_env = dict(os.environ, LIN_BENCH_INSERT_RUSAGE="1")
base_env.pop("LD_PRELOAD", None)
base_env.pop("MALLOC_CONF", None)
records = []
for rows in (10_000, 100_000):
    request = {"insert_rows": rows, "samples": 5, "timestamp_ms": 123456}
    reference = None
    allocators = ("system", "tcmalloc")
    for trial in range(6):
        for allocator in allocators[trial % 2:] + allocators[:trial % 2]:
            env = dict(base_env)
            if allocator == "tcmalloc":
                env["LD_PRELOAD"] = os.environ["LIN_BENCH_TCMALLOC"]
            data = json.loads(subprocess.check_output(
                [worker.resolve()], input=json.dumps(request) + "\n", text=True, env=env,
            ))
            assert len(data["result"]) == rows
            if reference is None:
                reference = data["result"]
            assert data["result"] == reference
            stats = data["insert_usage"]
            assert len(stats) == len(data["samples_ns"]) == request["samples"]
            for timing, stat in zip(data["samples_ns"], stats):
                assert timing > 0 and min(stat.values()) >= 0
                stat["wall_ms"] = timing / 1e6
            records.append({"rows": rows, "trial": trial, "allocator": allocator, "samples": stats})
            output.write_text(json.dumps(records, indent=2) + "\n")
    for allocator in allocators:
        trials = [r["samples"] for r in records if r["rows"] == rows and r["allocator"] == allocator]
        for label, stats in [("first", [s[0] for s in trials]), ("later", [t for s in trials for t in s[1:]])]:
            print(rows, allocator, label, {k: round(median(s[k] for s in stats), 2) for k in stats[0]}, flush=True)
