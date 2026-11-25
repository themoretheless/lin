"""Collect paired phase diagnostics on the same filesystem and CPU."""
import itertools
import json
import os
from pathlib import Path
import random
import runpy
from statistics import median
import subprocess
import sys
import tempfile

worker = Path(sys.argv[1]).resolve()
bench = runpy.run_path(str(Path(__file__).with_name("bench-wal-full.py")))
roles = ("base", "wide", "sqlite")
rng = random.Random(2929273)
output = Path("wal-9v74.json")
samples = []
for n in (1000, 10000):
    rows = bench["fixture"](n)
    orders = list(itertools.permutations(roles)) * 2
    rng.shuffle(orders)
    for order in orders:
        results = {}
        for role in order:
            with tempfile.TemporaryDirectory() as path:
                if role == "sqlite":
                    results[role] = {"total": bench["run_sqlite"](path, rows)}
                    continue
                env = dict(os.environ)
                flags = ("LIN_BENCH_SERIAL_WAL_VECTORS", "LIN_BENCH_AVX2_SERIAL", "LIN_BENCH_AVX2_WIDE")
                for flag in flags:
                    env.pop(flag, None)
                    if role == "wide":
                        env[flag] = "1"
                run = subprocess.run([worker, str(n), path], env=env, text=True, capture_output=True)
                if run.returncode:
                    raise RuntimeError(run.stderr)
                lines = [line.split() for line in run.stderr.splitlines()]
                build, = [p for p in lines if p[:2] == ["INSERT_BUILD", str(n)]]
                critical, = [p for p in lines if p[:2] == ["WAL_CRITICAL", str(n)]]
                encode, = [p for p in lines if p[:2] == ["WAL_ENCODE", str(n)] and p[3] == "true"]
                frame, = [p for p in lines if p[:2] == ["WAL_FRAME", encode[4]] and p[4] == "Full"]
                results[role] = dict(total=float(run.stdout), build=int(build[2]),
                                     maintain=int(critical[4]), wait=int(critical[5]),
                                     encode=int(encode[2]), write=int(frame[2]), sync=int(frame[3]))
        samples.append(dict(rows=n, order=order, results=results))
        output.write_text(json.dumps(samples, indent=2) + "\n")
    same_size = [x["results"] for x in samples if x["rows"] == n]
    for role in roles:
        print(n, role, {k: median(x[role][k] for x in same_size) / 1e6 for k in same_size[0][role]})
