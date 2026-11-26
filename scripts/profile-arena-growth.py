"""Compare glibc arena growth padding with default Linux pages, within host."""
import json
import os
import subprocess
import sys

rows = []
for size in (10_000, 100_000):
    request = json.dumps(dict(insert_rows=size, samples=3, timestamp_ms=123456))
    for trial in range(8):
        runs = ("default", "padded") if trial % 2 == 0 else ("padded", "default")
        control = None
        for setting in runs:
            env = dict(os.environ, LIN_BENCH_INSERT_RUSAGE="1")
            env.pop("GLIBC_TUNABLES", None)
            if setting == "padded":
                env["GLIBC_TUNABLES"] = "glibc.malloc.top_pad=2097152"
            data = json.loads(subprocess.check_output(
                [sys.argv[1]], input=request + "\n", text=True, env=env,
                stderr=subprocess.DEVNULL))
            assert len(data["result"]) == size
            assert len(data["samples_ns"]) == len(data["insert_usage"]) == 3
            if control is None:
                control = data["result"]
            else:
                assert control == data["result"]
            rows.append(dict(size=size, trial=trial, setting=setting,
                             times=data["samples_ns"], usage=data["insert_usage"]))
print(json.dumps(rows))
