"""Compare prepared and column-batch Lin against SQLite WAL FULL."""
from itertools import permutations
import json
from pathlib import Path
import random
import runpy
from statistics import median
import subprocess
import sys
import tempfile

worker, output = map(Path, sys.argv[1:])
worker = worker.resolve()
bench = runpy.run_path(str(Path(__file__).with_name("bench-wal-full.py")))
roles = ("prepared", "prepared_control", "column_batch", "sqlite")
rng = random.Random(2929273)
report = {"samples": []}
for count in (1000, 10000):
    rows = bench["fixture"](count)
    orders = list(permutations(roles))
    rng.shuffle(orders)
    for order in orders:
        timings = {}
        for role in order:
            with tempfile.TemporaryDirectory() as directory:
                if role == "sqlite":
                    timings[role] = bench["run_sqlite"](directory, rows)
                    continue
                req = dict(insert_rows=count, samples=1, timestamp_ms=123456,
                           durable_dir=directory, column_batch=role == "column_batch")
                result = json.loads(subprocess.check_output(
                    [worker], input=json.dumps(req) + "\n", text=True,
                ))
                assert [tuple(row) for row in result["result"]] == rows
                timings[role] = result["samples_ns"][0]
        report["samples"].append(dict(rows=count, order=order, ns=timings))
        output.write_text(json.dumps(report, indent=2) + "\n")
    times = [s["ns"] for s in report["samples"] if s["rows"] == count]
    assert len(times) == 24
    print(count, {r: median(t[r] for t in times)/1e6 for r in roles})
    print("batch gain vs prepared:", median(1 - t["column_batch"]/t["prepared"] for t in times))
    print("prepared A/A:", median(abs(1 - t["prepared_control"]/t["prepared"]) for t in times))
    print("wins vs SQLite:", {r: sum(t[r] < t["sqlite"] for t in times) for r in roles[:3]}, "/24")
