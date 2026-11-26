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


def main(worker, output):
    roles = ("lin", "control", "duckdb")
    report = dict(
        duckdb=duckdb.__version__, pandas=pd.__version__, host=platform.platform(),
        worker_sha256=hashlib.sha256(worker.read_bytes()).hexdigest(), samples=[],
    )
    rng = random.Random(2929273)
    for n in (10_000, 100_000):
        rows = bench["fixture"](n)
        frame = pd.DataFrame(rows, columns=FIELDS)
        measure_duckdb(frame, rows)
        bench["run_lin"](worker, None, rows)
        for block in range(4):
            orders = list(itertools.permutations(roles))
            rng.shuffle(orders)
            for order in orders:
                timings = {}
                for role in order:
                    timings[role] = (
                        measure_duckdb(frame, rows) if role == "duckdb"
                        else bench["run_lin"](worker, None, rows)
                    )
                report["samples"].append(dict(rows=n, block=block, order=order, ns=timings))
                output.write_text(json.dumps(report, indent=2) + "\n")
        pairs = [x["ns"] for x in report["samples"] if x["rows"] == n]
        timings = {role: median(x[role] for x in pairs) / 1e6 for role in roles}
        wins = sum(x["lin"] < x["duckdb"] for x in pairs)
        noise = median(abs(1 - x["control"] / x["lin"]) for x in pairs)
        gain = median(1 - x["lin"] / x["duckdb"] for x in pairs)
        print(f"{n} rows {timings}, Lin wins {wins}/{len(pairs)}, paired gain {gain:.2%}, A/A {noise:.2%}", flush=True)


if __name__ == "__main__":
    main(Path(sys.argv[1]).resolve(), Path(sys.argv[2]))
