"""Time stages inside the same durable INSERT fixtures as the paired qualification."""
import json
from pathlib import Path
import random
import runpy
import sqlite3
import subprocess
import sys
import tempfile
from time import perf_counter_ns

bench = runpy.run_path(str(Path(__file__).with_name("bench-wal-full.py")))


def sqlite_stage(path, rows):
    db = sqlite3.connect(path, isolation_level=None)
    try:
        assert db.execute("PRAGMA journal_mode=WAL").fetchone() == ("wal",)
        db.execute("PRAGMA synchronous=FULL")
        assert db.execute("PRAGMA synchronous").fetchone() == (2,)
        db.executescript(bench["SCHEMA"])
        start = perf_counter_ns()
        db.execute("BEGIN")
        db.executemany("INSERT INTO docs VALUES (?,?,?,?,?,?)", rows)
        before_commit = perf_counter_ns()
        db.execute("COMMIT")
        end = perf_counter_ns()
    finally:
        db.close()
    db = sqlite3.connect(path)
    try:
        assert sorted(db.execute("SELECT * FROM docs")) == sorted(rows)
    finally:
        db.close()
    return dict(insert_ns=before_commit-start, commit_ns=end-before_commit)


def lin_stage(path, rows):
    request = dict(insert_rows=len(rows), samples=1, timestamp_ms=123456, durable_dir=str(path))
    p = subprocess.run([sys.argv[1]], input=json.dumps(request), capture_output=True, text=True, check=True)
    result = json.loads(p.stdout)
    assert result["reopen_ns"][0] > 0 and [tuple(r) for r in result["result"]] == rows
    trace = p.stderr.split("BEGIN INSERT\n")[1].split("END INSERT\n")[0]
    events = [json.loads(line) for line in trace.splitlines()]
    phases = [r for r in events if "phase" in r]
    spans = {r["span"]: r for r in events if "span" in r}
    assert set(spans) == {"encoding", "maintenance"}, events
    assert [r["phase"] for r in phases] == ["build", "validate", "maintain", "append", "wal_io"], phases
    return dict(total_ns=result["samples_ns"][0], phases=phases, spans=spans)


def main():
    rng = random.Random(20261127)
    for n in (1000, 10000):
        rows = bench["fixture"](n)
        orders = [("sqlite", "lin"), ("lin", "sqlite")] * int(sys.argv[2])
        rng.shuffle(orders)
        for order in orders:
            sample = dict(n=n, order=order, sqlite_version=sqlite3.sqlite_version)
            for role in order:
                with tempfile.TemporaryDirectory() as d:
                    sample[role] = sqlite_stage(Path(d)/"db", rows) if role == "sqlite" else lin_stage(Path(d), rows)
            print(json.dumps(sample), flush=True)


if __name__ == "__main__":
    main()
