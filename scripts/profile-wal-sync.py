"""Measure FULL WAL syncs without changing durability or WAL encoding."""
import json
import os
from pathlib import Path
import random
import runpy
import subprocess
import sys
import tempfile

bench = Path(__file__).with_name("bench-wal-full.py")
worker, tracer, output = (Path(value).resolve() for value in sys.argv[1:])
rng = random.Random(2929273)
samples = []
for n in (1000, 10000):
    orders = [("lin", "sqlite"), ("sqlite", "lin")] * 12
    rng.shuffle(orders)
    for order in orders:
        result = {}
        for engine in order:
            with tempfile.TemporaryDirectory() as directory:
                env = dict(os.environ, LD_PRELOAD=str(tracer))
                if engine == "lin":
                    for flag in ("SERIAL_WAL_VECTORS", "AVX2_SERIAL", "AVX2_WIDE"):
                        env[f"LIN_BENCH_{flag}"] = "1"
                    cmd = [worker]
                    stdin = json.dumps(dict(insert_rows=n, samples=1, timestamp_ms=123456,
                                            durable_dir=directory)) + "\n"
                else:
                    cmd = [sys.executable, "-c",
                           "import runpy,sys; b=runpy.run_path(sys.argv[1]); "
                           "print(b['run_sqlite'](sys.argv[2],b['fixture'](int(sys.argv[3]))))",
                           bench, directory, str(n)]
                    stdin = None
                proc = subprocess.run(cmd, input=stdin, env=env, text=True, capture_output=True)
                if proc.returncode:
                    raise RuntimeError(proc.stderr)
                if engine == "lin":
                    data = json.loads(proc.stdout)
                    assert [tuple(row) for row in data["result"]] == runpy.run_path(str(bench))["fixture"](n)
                    elapsed = data["samples_ns"][0]
                else:
                    elapsed = int(proc.stdout)
                syncs = []
                for line in proc.stderr.splitlines():
                    if line.startswith("WAL_SYNC "):
                        _, operation, code, size, ns, path = line.split(" ", 5)
                        assert code == "0", line
                        if Path(path).parent == Path(directory):
                            syncs.append(dict(operation=operation, file=Path(path).name,
                                              bytes=int(size), ns=int(ns)))
                assert syncs, (engine, proc.stderr)
                result[engine] = dict(insert_ns=elapsed, syncs=syncs)
        samples.append(dict(rows=n, order=order, result=result))
        output.write_text(json.dumps(samples, indent=2) + "\n")
