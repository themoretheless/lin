"""Compare prepared in-memory batch INSERT with exact readback."""
import hashlib
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


def role_env(role):
    env = dict(os.environ)
    env.pop("LD_PRELOAD", None)
    env.pop("MALLOC_CONF", None)
    env.pop("GLIBC_TUNABLES", None)
    if role in ("tcmalloc", "duckdb_tcmalloc"):
        env["LD_PRELOAD"] = env["LIN_BENCH_TCMALLOC"]
    return env


def measure_lin(worker, rows, samples, role):
    request = dict(insert_rows=len(rows), samples=samples, timestamp_ms=123456)
    result = json.loads(subprocess.check_output(
        [worker], input=json.dumps(request) + "\n", text=True, env=role_env(role)))
    if [tuple(row) for row in result["result"]] != rows:
        raise ValueError("Lin readback disagrees with fixture")
    timings = result["samples_ns"]
    if len(timings) != samples or min(timings) <= 0:
        raise ValueError("Lin returned incomplete sample timings")
    return timings


def main(worker, output):
    roles = ("lin", "control", "tcmalloc", "duckdb", "duckdb_tcmalloc")
    samples = 5
    rng = random.Random(2929273)
    report = dict(
        duckdb=duckdb.__version__, pandas=pd.__version__, host=platform.platform(),
        worker_sha256=hashlib.sha256(worker.read_bytes()).hexdigest(),
        samples=[],
    )
    for n in (10_000, 100_000):
        rows = bench["fixture"](n)
        orders = []
        for _ in range(5):
            cycle = rng.sample(roles, len(roles))
            orders.extend(cycle[i:] + cycle[:i] for i in range(len(roles)))
        rng.shuffle(orders)
        for order in orders:
            timings = {}
            for role in order:
                if role.startswith("duckdb"):
                    timings[role] = json.loads(subprocess.check_output(
                        [sys.executable, __file__, "--duckdb", str(n), str(samples)],
                        text=True, env=role_env(role)))
                else:
                    timings[role] = measure_lin(worker, rows, samples, role)
            report["samples"].append(dict(rows=n, order=order, ns=timings))
            output.write_text(json.dumps(report, indent=2) + "\n")
        pairs = [sample["ns"] for sample in report["samples"] if sample["rows"] == n]
        for label, aggregate in (("first", lambda ts: ts[0]), ("warm", lambda ts: median(ts[1:]))):
            ms = {role: median(aggregate(pair[role]) for pair in pairs) / 1e6 for role in roles}
            wins = sum(aggregate(pair["lin"]) < aggregate(pair["duckdb"]) for pair in pairs)
            allocator_wins = sum(aggregate(pair["tcmalloc"]) < aggregate(pair["duckdb_tcmalloc"]) for pair in pairs)
            print(f"{n} rows {label}: {ms}, Lin wins {wins}/{len(pairs)}, "
                  f"Lin TCMalloc wins {allocator_wins}/{len(pairs)}", flush=True)


if __name__ == "__main__":
    if sys.argv[1] == "--duckdb":
        rows = bench["fixture"](int(sys.argv[2]))
        frame = pd.DataFrame(rows, columns=FIELDS)
        print(json.dumps([measure_duckdb(frame, rows) for _ in range(int(sys.argv[3]))]))
        sys.exit(0)
    main(*(Path(arg).resolve() for arg in sys.argv[1:]))
