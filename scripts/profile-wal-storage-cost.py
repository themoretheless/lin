"""Compare identical FULL commits on disk and tmpfs to isolate storage sensitivity."""
import json
from itertools import product
from pathlib import Path
import random
import runpy
import sqlite3
import sys
import tempfile

bench = runpy.run_path(str(Path(__file__).with_name("bench-wal-full.py")))
worker = Path(sys.argv[1]).resolve()
report = dict(sqlite=sqlite3.sqlite_version, samples=[])
randomizer = random.Random(81991)
for rows in (1000, 10000):
    fixture = bench["fixture"](rows)
    orders = []
    for _ in range(4):
        cycle = list(product(("lin", "sqlite"), ("disk", "tmpfs")))
        randomizer.shuffle(cycle)
        orders.extend(cycle[i:] + cycle[:i] for i in range(4))
    randomizer.shuffle(orders)
    for order in orders:
        timings = {}
        frames = {}
        for engine, storage in order:
            root = "/dev/shm" if storage == "tmpfs" else tempfile.gettempdir()
            with tempfile.TemporaryDirectory(dir=root) as directory:
                if engine == "lin":
                    duration, frames["lin_" + storage] = bench["run_lin"](worker, directory, fixture, profile=True)
                else:
                    duration, frames["sqlite_growth_" + storage] = bench["run_sqlite"](directory, fixture, profile=True)
            timings[f"{engine}_{storage}"] = duration
        report["samples"].append(dict(rows=rows, order=order, ns=timings, frames=frames))
        Path(sys.argv[2]).write_text(json.dumps(report))
