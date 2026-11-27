"""Run Rust timers in separate processes on the same host and filesystem."""
from itertools import permutations
import json
from pathlib import Path
import subprocess
from tempfile import TemporaryDirectory

workers = {
    "baseline": ("typed-baseline", "lin-wal"),
    "two_pass": ("typed-two-pass", "lin-wal"),
    "prefix": ("embedded-native", "lin-wal"),
    "sqlite": ("embedded-native", "sqlite"),
}
orders = list(permutations(workers))
results = []
for size in (1000, 10000):
    for i in range(24):
        order = orders[i % len(orders)]
        times = {}
        for worker in order:
            binary, mode = workers[worker]
            with TemporaryDirectory() as directory:
                output = subprocess.check_output(
                    [f"target/release/{binary}", mode, str(size), "1", directory],
                    text=True,
                )
            elapsed, = json.loads(output)
            assert elapsed > 0
            times[worker] = elapsed
        results.append(dict(size=size, order=order, times_ns=times))
        Path("typed-wal-four-way.json").write_text(json.dumps(results))
