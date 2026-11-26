"""Record transaction begin, ten prepared inserts, and commit separately."""
import json
import os
import subprocess
import sys

request = json.dumps(dict(insert_rows=100000, samples=3, timestamp_ms=123456))
results = []
for trial in range(8):
    for mode in (("default", "thp") if trial % 2 == 0 else ("thp", "default")):
        env = dict(os.environ, LIN_BENCH_INSERT_STAGES="1")
        env.pop("GLIBC_TUNABLES", None)
        if mode == "thp":
            env["GLIBC_TUNABLES"] = "glibc.malloc.hugetlb=1"
        data = json.loads(subprocess.check_output([sys.argv[1]], text=True, input=request, env=env))
        assert len(data["result"]) == 100000
        ends = data["stage_end_ns"]
        assert len(ends) == 3 and all(len(sample) == 12 for sample in ends)
        durations = [[b-a for a,b in zip([0]+sample, sample)] for sample in ends]
        assert all(min(sample) >= 0 for sample in durations)
        results.append(dict(trial=trial, mode=mode, stage_ns=durations, total_ns=data["samples_ns"]))
print(json.dumps(results))
