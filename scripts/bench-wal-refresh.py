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
        roles = ("base", "control", "wide", "candidate", "sqlite")
        rank = {role: i for i, role in enumerate(roles)}
        orders = [o for o in permutations(roles)
                  if sum(rank[a] > rank[b] for i, a in enumerate(o) for b in o[i+1:]) % 2 == 0]
        rng.shuffle(orders)
        for order in orders:
            results = {}
            hashes = set()
            for role in order:
                with tempfile.TemporaryDirectory() as path:
                    if role == "sqlite":
                        results[role] = {"insert_ns": module.run_sqlite(path, rows)}
                        continue
                    env = dict(os.environ)
                    fast_flags = (
                        "LIN_BENCH_SERIAL_WAL_VECTORS",
                        "LIN_BENCH_AVX2_SERIAL",
                        "LIN_BENCH_AVX2_WIDE",
                    )
                    for flag in (*fast_flags, "LIN_BENCH_WAL_DICTIONARY", "LIN_BENCH_DIRECT_WAL_LARGE", "LIN_BENCH_WAL_DICT_AVX2", "LIN_BENCH_WAL_DICT_PACK"):
                        env.pop(flag, None)
                    if role in ("wide", "candidate"):
                        for flag in fast_flags:
                            env[flag] = "1"
                    if role == "candidate":
                        if n >= 4096 or platform.machine() == "x86_64":
                            env["LIN_BENCH_WAL_DICTIONARY"] = "1"
                        if platform.machine() == "x86_64":
                            env["LIN_BENCH_WAL_DICT_AVX2"] = "1"
                            env["LIN_BENCH_WAL_DICT_PACK"] = "1"
                    req = dict(insert_rows=n, samples=1, timestamp_ms=123456, durable_dir=path)
                    data = json.loads(subprocess.check_output([worker], input=json.dumps(req)+"\n", text=True, env=env))
                    assert [tuple(row) for row in data["result"]] == rows
                    assert data["embedding_hash"] is not None
                    hashes.add(data["embedding_hash"])
                    results[role] = {k: data[src][0] for k, src in [
                        ("insert_ns", "samples_ns"), ("open_ns", "open_ns"), ("reopen_ns", "reopen_ns"), ("wal_bytes", "wal_bytes")]}
            assert len(hashes) == 1, "WAL replay embeddings differ"
            report["samples"].append(dict(rows=n, block=block, order=order, measurements=results))
            report["load_latest"] = os.getloadavg()
            temporary = output.with_suffix(".tmp")
            temporary.write_text(json.dumps(report, indent=2) + "\n")
            temporary.replace(output)
