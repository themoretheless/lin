"""Measure transaction boundaries in fresh worker processes, with warmed repeats."""
import json
from pathlib import Path
import subprocess

report = []
for _ in range(12):
    request = dict(insert_rows=100000, samples=5, timestamp_ms=123456)
    reply = json.loads(subprocess.check_output(
        ["target/release/lin-peer-worker"], input=json.dumps(request) + "\n", text=True))
    assert len(reply["result"]) == 100000
    phases = []
    for bounds, total in zip(reply["boundaries_ns"], reply["samples_ns"], strict=True):
        assert len(bounds) == 12 and bounds[-1] == total
        parts = [b - a for a, b in zip([0] + bounds[:-1], bounds)]
        assert min(parts) > 0
        phases.append(parts)
    assert len(phases) == 5
    report.append(phases)
Path("chunks.json").write_text(json.dumps(report))
