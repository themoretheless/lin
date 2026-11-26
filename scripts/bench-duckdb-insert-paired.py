"""Compare prepared in-memory batch INSERT with exact readback."""
import hashlib
import itertools
import json
from pathlib import Path
import platform
import random
import runpy
from statistics import median
import sys
import subprocess
from time import perf_counter_ns

import duckdb
import pandas as pd

bench = runpy.run_path(str(Path(__file__).with_name("bench-wal-full.py")))
FIELDS = ("id", "uri", "wing", "title", "ts", "body")


def measure_duckdb(frame, rows):
    with duckdb.connect(":memory:") as db:
        db.execute(bench["SCHEMA"].replace("ts INTEGER", "ts BIGINT"))
        db.register("input_docs", frame)
        db.execute("PREPARE ins AS INSERT INTO docs SELECT id, uri, wing, title, ts, body FROM input_docs")
        start = perf_counter_ns()
        db.execute("BEGIN TRANSACTION")
        db.execute("EXECUTE ins")
        db.commit()
        elapsed = perf_counter_ns() - start
        if sorted(db.execute("SELECT * FROM docs").fetchall()) != sorted(rows):
            raise ValueError("DuckDB readback disagrees with fixture")
        return elapsed


def measure_lin(worker, rows):
    request = dict(insert_rows=len(rows), samples=1, timestamp_ms=123456)
    result = json.loads(subprocess.check_output(
        [worker], input=json.dumps(request) + "\n", text=True))
    if [tuple(row) for row in result["result"]] != rows:
        raise ValueError("Lin readback disagrees with fixture")
    return result["samples_ns"][0]


def main(worker, native_worker, output):
    roles = ("lin", "control", "native_cpu", "duckdb")
    report = dict(
        duckdb=duckdb.__version__, pandas=pd.__version__, host=platform.platform(),
        worker_sha256=hashlib.sha256(worker.read_bytes()).hexdigest(),
        native_sha256=hashlib.sha256(native_worker.read_bytes()).hexdigest(), samples=[],
    )
    rng = random.Random(2929273)
    for n in (10_000, 100_000):
        rows = bench["fixture"](n)
        frame = pd.DataFrame(rows, columns=FIELDS)
        measure_duckdb(frame, rows)
        measure_lin(worker, rows)
        measure_lin(native_worker, rows)
        for block in range(1):
            orders = list(itertools.permutations(roles))
            rng.shuffle(orders)
            for order in orders:
                timings = {}
                for role in order:
                    timings[role] = (
                        measure_duckdb(frame, rows) if role == "duckdb"
                        else measure_lin(native_worker if role == "native_cpu" else worker, rows)
                    )
                report["samples"].append(dict(rows=n, block=block, order=order, ns=timings))
                output.write_text(json.dumps(report, indent=2) + "\n")
        pairs = [x["ns"] for x in report["samples"] if x["rows"] == n]
        timings = {role: median(x[role] for x in pairs) / 1e6 for role in roles}
        wins = sum(x["lin"] < x["duckdb"] for x in pairs)
        noise = median(abs(1 - x["control"] / x["lin"]) for x in pairs)
        gain = median(1 - x["lin"] / x["duckdb"] for x in pairs)
        native_cpu_gain = median(1 - x["native_cpu"] / x["lin"] for x in pairs)
        native_cpu_wins = sum(x["native_cpu"] < x["duckdb"] for x in pairs)
        print(f"{n} rows {timings}, Lin wins {wins}/{len(pairs)}, paired gain {gain:.2%}, "
              f"A/A {noise:.2%}, native_cpu gain {native_cpu_gain:.2%}, "
              f"native_cpu wins {native_cpu_wins}/{len(pairs)}", flush=True)


if __name__ == "__main__":
    main(*(Path(arg).resolve() for arg in sys.argv[1:]))
