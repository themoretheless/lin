"""Run Rust timers in separate processes on the same host and filesystem."""
from itertools import permutations
import json
from pathlib import Path
import subprocess
from tempfile import TemporaryDirectory

orders = list(permutations(("lin-memory", "lin-wal", "sqlite-memory", "sqlite")))
results = []
for size in (1000, 10000):
    for i in range(24):
        order = orders[i % len(orders)]
        times = {}
        for worker in order:
            mode = worker
            with TemporaryDirectory() as directory:
                output = subprocess.check_output(
                    ["target/release/embedded-native", mode, str(size), "1", directory],
                    text=True,
                )
            elapsed, = json.loads(output)
            assert elapsed > 0
            times[worker] = elapsed
        results.append(dict(size=size, order=order, times_ns=times))
        Path("typed-storage-modes.json").write_text(json.dumps(results))
