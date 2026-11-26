"""Compare prepared in-memory batch INSERT with exact readback."""
import hashlib
import itertools
import json
import os
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


def measure_lin(worker, rows, reserve_terms=False):
    request = dict(insert_rows=len(rows), samples=1, timestamp_ms=123456)
    env = dict(os.environ)
    env["LIN_BENCH_FTS_FUSED_ASCII"] = "1"
    env.pop("LIN_BENCH_FTS_RESERVE_TERMS", None)
    if reserve_terms:
        env["LIN_BENCH_FTS_RESERVE_TERMS"] = "1"
    result = json.loads(subprocess.check_output(
        [worker], input=json.dumps(request) + "\n", text=True, env=env))
    if [tuple(row) for row in result["result"]] != rows:
        raise ValueError("Lin readback disagrees with fixture")
    return result["samples_ns"][0], result["embedding_hash"]


def main(worker, output):
    roles = ("lin", "control", "reserve_terms", "duckdb")
    report = dict(
        duckdb=duckdb.__version__, pandas=pd.__version__, host=platform.platform(),
        worker_sha256=hashlib.sha256(worker.read_bytes()).hexdigest(),
        samples=[],
    )
    rng = random.Random(2929273)
    for n in (10_000, 100_000):
        rows = bench["fixture"](n)
        frame = pd.DataFrame(rows, columns=FIELDS)
        measure_duckdb(frame, rows)
        measure_lin(worker, rows)
        measure_lin(worker, rows, True)
        for block in range(1):
            orders = list(itertools.permutations(roles))
            rng.shuffle(orders)
            for order in orders:
                timings = {}
                hashes = set()
                for role in order:
                    if role == "duckdb":
                        timings[role] = measure_duckdb(frame, rows)
                    else:
                        timings[role], digest = measure_lin(worker, rows, role == "reserve_terms")
                        hashes.add(digest)
                assert len(hashes) == 1, "Lin computed fields differ between INSERT APIs"
                report["samples"].append(dict(rows=n, block=block, order=order, ns=timings))
                output.write_text(json.dumps(report, indent=2) + "\n")
        pairs = [x["ns"] for x in report["samples"] if x["rows"] == n]
        timings = {role: median(x[role] for x in pairs) / 1e6 for role in roles}
        wins = sum(x["lin"] < x["duckdb"] for x in pairs)
        noise = median(abs(1 - x["control"] / x["lin"]) for x in pairs)
        gain = median(1 - x["lin"] / x["duckdb"] for x in pairs)
        reserve_terms_gain = median(1 - x["reserve_terms"] / x["lin"] for x in pairs)
        reserve_terms_wins = sum(x["reserve_terms"] < x["duckdb"] for x in pairs)
        print(f"{n} rows {timings}, Lin wins {wins}/{len(pairs)}, paired gain {gain:.2%}, "
              f"A/A {noise:.2%}, reserve_terms gain {reserve_terms_gain:.2%}, "
              f"reserve_terms wins {reserve_terms_wins}/{len(pairs)}", flush=True)


if __name__ == "__main__":
    main(*(Path(arg).resolve() for arg in sys.argv[1:]))
