"""Paired Lin/SQLite WAL FULL benchmark with an independent Lin A/A control."""
import argparse
from itertools import permutations
import json
from pathlib import Path
import random
import sqlite3
from statistics import median
import subprocess
import tempfile
from time import perf_counter_ns

SCHEMA = """CREATE TABLE docs (
    id TEXT PRIMARY KEY, uri TEXT UNIQUE, wing TEXT, title TEXT, ts INTEGER, body TEXT
);
CREATE INDEX docs_wing_ts ON docs (wing, ts);"""


def fixture(n):
    return [(f"d-{i}", f"bench://{i}", "rag" if i % 2 == 0 else "sys",
             f"doc {i} " + ("wal note" if i % 10 == 0 else "plain"),
             123456, f"body {i}") for i in range(n)]


def run_lin(worker, directory, rows, profile=False):
    request = dict(insert_rows=len(rows), samples=1, timestamp_ms=123456, durable_dir=directory)
    proc = subprocess.run([worker], input=json.dumps(request) + "\n", text=True, capture_output=True, check=True)
    result = json.loads(proc.stdout)
    if profile:
        measured = proc.stderr.split("frame_profile_begin\n")[1].split("frame_profile_end\n")[0]
        frames = [line.split() for line in measured.splitlines() if line.startswith("frame_cost ")]
        if len(frames) != 1 or frames[0][-1] != "Full":
            raise ValueError(f"Unexpected measured WAL frames: {frames}")
        phases = dict((key, int(value)) for _, key, value in (line.split() for line in measured.splitlines() if line.startswith("insert_phase ")))
        if len(phases) != 4: raise ValueError(f"missing phases: {phases}")
        frame = dict(zip(("bytes", "write_ns", "sync_ns"), map(int, frames[0][1:4])))
        frame["phases"] = phases
        frame["row_samples"] = [list(map(int, line.split()[1:])) for line in measured.splitlines()
                                if line.startswith("row_samples ")]
        if sum(r[0] for r in frame["row_samples"]) < len(rows) // 40:
            raise ValueError("Incomplete sampled rows")
    if not (len(result.get("reopen_ns", [])) == 1 and result["reopen_ns"][0] > 0
            and any(Path(directory).iterdir())):
        raise ValueError("Lin worker did not confirm a durable database reopen")
    if [tuple(row) for row in result["result"]] != rows:
        raise ValueError("Lin durable INSERT readback disagrees with fixture")
    return (result["samples_ns"][0], frame) if profile else result["samples_ns"][0]


def run_sqlite(directory, rows, profile=False):
    path = Path(directory) / "db.sqlite"
    db = sqlite3.connect(path, isolation_level=None)
    try:
        if db.execute("PRAGMA journal_mode=WAL").fetchone() != ("wal",):
            raise RuntimeError("SQLite refused WAL mode")
        db.execute("PRAGMA synchronous=FULL")
        if db.execute("PRAGMA synchronous").fetchone() != (2,):
            raise RuntimeError("SQLite refused FULL sync")
        db.executescript(SCHEMA)
        if profile:
            wal_path = Path(str(path) + "-wal")
            wal_before = wal_path.stat().st_size
        start = perf_counter_ns()
        db.execute("BEGIN")
        db.executemany("INSERT INTO docs VALUES (?,?,?,?,?,?)", rows)
        db.execute("COMMIT")
        elapsed = perf_counter_ns() - start
        if profile:
            wal_growth = wal_path.stat().st_size - wal_before
    finally:
        db.close()
    db = sqlite3.connect(path)
    try:
        if sorted(db.execute("SELECT * FROM docs").fetchall()) != sorted(rows):
            raise ValueError("SQLite durable INSERT readback disagrees with fixture")
    finally:
        db.close()
    return (elapsed, wal_growth) if profile else elapsed


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("worker", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--control", type=Path, help="Optional reference Lin worker")
    parser.add_argument("--blocks", type=int, default=4)
    args = parser.parse_args()
    if args.blocks < 1:
        parser.error("--blocks must be positive")
    workers = {"lin": args.worker.resolve(), "lin_control": (args.control or args.worker).resolve()}
    rng = random.Random(2929273)
    report = dict(sqlite=sqlite3.sqlite_version, samples=[])
    for n in (1000, 10000):
        rows = fixture(n)
        for block in range(args.blocks):
            orders = list(permutations(("lin", "lin_control", "sqlite")))
            rng.shuffle(orders)
            for order in orders:
                samples = {}
                for role in order:
                    with tempfile.TemporaryDirectory() as directory:
                        elapsed = (run_sqlite(directory, rows) if role == "sqlite"
                                   else run_lin(workers[role], directory, rows))
                    if elapsed <= 0:
                        raise ValueError(f"Nonpositive measurement: {role}, {elapsed}")
                    samples[role] = elapsed
                report["samples"].append(dict(rows=n, block=block, order=order, ns=samples))
                args.output.write_text(json.dumps(report, indent=2) + "\n")
        pairs = [row["ns"] for row in report["samples"] if row["rows"] == n]
        print(n, {role: median(row[role] for row in pairs) / 1e6 for role in orders[0]},
              "Lin wins", sum(row["lin"] < row["sqlite"] for row in pairs), "/", len(pairs),
              "median absolute Lin/control %", median(abs(100 * (1 - row["lin_control"] / row["lin"])) for row in pairs))


if __name__ == "__main__":
    main()
