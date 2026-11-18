import hashlib
import importlib.util
import json
import os
from pathlib import Path
import random
import platform
import subprocess
import sys
import tempfile
from itertools import permutations

worker, bench, output = map(Path, sys.argv[1:])
spec = importlib.util.spec_from_file_location("bench", bench)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
rng = random.Random(2929273)
report = {
    "sqlite": module.sqlite3.sqlite_version,
    "host": platform.platform(),
    "worker_sha256": hashlib.sha256(worker.read_bytes()).hexdigest(),
    "bench_sha256": hashlib.sha256(bench.read_bytes()).hexdigest(),
    "runner_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
    "load_start": os.getloadavg(),
    "samples": [],
}
for n in (1000, 10000):
    rows = module.fixture(n)
    for block in range(1):
        orders = list(permutations(("base", "control", "serial", "compact", "sqlite")))
        rng.shuffle(orders)
        orders = [o for o in orders if o.index("base") < o.index("control")]
        for order in orders:
            results = {}
            for role in order:
                with tempfile.TemporaryDirectory() as path:
                    if role == "sqlite":
                        results[role] = {"insert_ns": module.run_sqlite(path, rows)}
                        continue
                    env = dict(os.environ)
                    env.pop("LIN_BENCH_SERIAL_WAL_VECTORS", None)
                    env.pop("LIN_BENCH_DIRECT_EMBED_PARTS", None)
                    if role in ("serial", "compact"):
                        env["LIN_BENCH_SERIAL_WAL_VECTORS"] = "1"
                    if role == "compact":
                        env["LIN_BENCH_DIRECT_EMBED_PARTS"] = "1"
                    req = dict(insert_rows=n, samples=1, timestamp_ms=123456, durable_dir=path)
                    data = json.loads(subprocess.check_output([worker], input=json.dumps(req)+"\n", env=env, text=True))
                    assert [tuple(row) for row in data["result"]] == rows
                    results[role] = {k: data[src][0] for k, src in [
                        ("insert_ns", "samples_ns"), ("reopen_ns", "reopen_ns"), ("wal_bytes", "wal_bytes")]}
            report["samples"].append(dict(rows=n, block=block, order=order, measurements=results))
            report["load_latest"] = os.getloadavg()
            temporary = output.with_suffix(".tmp")
            temporary.write_text(json.dumps(report, indent=2) + "\n")
            temporary.replace(output)
