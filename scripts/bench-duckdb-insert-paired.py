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


def physical_core_cpus():
    allowed = os.sched_getaffinity(0)
    result = {}
    topology = subprocess.check_output(["lscpu", "-p=CPU,CORE,SOCKET"], text=True)
    for line in topology.splitlines():
        if line.startswith("#"):
            continue
        cpu, core, socket = map(int, line.split(","))
        if cpu in allowed:
            result.setdefault((socket, core), cpu)
    if not result:
        raise RuntimeError("No allowed CPUs in reported topology")
    return sorted(result.values())


def measure_lin(worker, rows, cpus=None):
    command = [worker]
    if cpus is not None:
        command = ["taskset", "-c", ",".join(map(str, cpus)), worker]
    request = dict(insert_rows=len(rows), samples=1, timestamp_ms=123456)
    result = json.loads(subprocess.check_output(
        command, input=json.dumps(request) + "\n", text=True))
    if [tuple(row) for row in result["result"]] != rows:
        raise ValueError("Lin readback disagrees with fixture")
    return result["samples_ns"][0]


def main(worker, output):
    roles = ("lin", "control", "physical", "duckdb")
    cpus = physical_core_cpus()
    report = dict(
        duckdb=duckdb.__version__, pandas=pd.__version__, host=platform.platform(),
        allowed_cpus=sorted(os.sched_getaffinity(0)), physical_core_cpus=cpus,
        worker_sha256=hashlib.sha256(worker.read_bytes()).hexdigest(), samples=[],
    )
    rng = random.Random(2929273)
    for n in (10_000, 100_000):
        rows = bench["fixture"](n)
        frame = pd.DataFrame(rows, columns=FIELDS)
        measure_duckdb(frame, rows)
        measure_lin(worker, rows)
        measure_lin(worker, rows, cpus)
        for block in range(1):
            orders = list(itertools.permutations(roles))
            rng.shuffle(orders)
            for order in orders:
                timings = {}
                for role in order:
                    timings[role] = (
                        measure_duckdb(frame, rows) if role == "duckdb"
                        else measure_lin(worker, rows, cpus if role == "physical" else None)
                    )
                report["samples"].append(dict(rows=n, block=block, order=order, ns=timings))
                output.write_text(json.dumps(report, indent=2) + "\n")
        pairs = [x["ns"] for x in report["samples"] if x["rows"] == n]
        timings = {role: median(x[role] for x in pairs) / 1e6 for role in roles}
        wins = sum(x["lin"] < x["duckdb"] for x in pairs)
        noise = median(abs(1 - x["control"] / x["lin"]) for x in pairs)
        gain = median(1 - x["lin"] / x["duckdb"] for x in pairs)
        physical_gain = median(1 - x["physical"] / x["lin"] for x in pairs)
        physical_wins = sum(x["physical"] < x["duckdb"] for x in pairs)
        print(f"{n} rows {timings}, Lin wins {wins}/{len(pairs)}, paired gain {gain:.2%}, "
              f"A/A {noise:.2%}, physical gain {physical_gain:.2%}, "
              f"physical wins {physical_wins}/{len(pairs)}", flush=True)


if __name__ == "__main__":
    main(Path(sys.argv[1]).resolve(), Path(sys.argv[2]))
