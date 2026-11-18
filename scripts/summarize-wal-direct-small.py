import json
import sys
from pathlib import Path
from statistics import median

raw = json.loads(Path(sys.argv[1]).read_text())
for n in (1000, 10000):
    samples = [x["measurements"] for x in raw["samples"] if x["rows"] == n]
    if len(samples) != 60:
        raise SystemExit(f"{n}: incomplete, {len(samples)}/60")
    print(f"{n} rows, {len(samples)} paired windows")
    for role in ("base", "control", "serial", "compact", "sqlite"):
        medians = {
            field: median(x[role][field] for x in samples)
            for field in samples[0][role]
        }
        print(role, medians)
    for metric in ("insert_ns", "reopen_ns"):
        aa = median(abs(x["control"][metric] / x["base"][metric] - 1) for x in samples)
        gain = median(1 - x["compact"][metric] / x["base"][metric] for x in samples)
        print(metric, f"paired gain {gain:.2%}, median A/A noise {aa:.2%}")
    gain = median(1 - x["compact"]["insert_ns"] / x["serial"]["insert_ns"] for x in samples)
    print("row-build-only paired gain", f"{gain:.2%}")
    for role in ("base", "control", "serial", "compact"):
        wins = sum(x[role]["insert_ns"] < x["sqlite"]["insert_ns"] for x in samples)
        print(f"{role} wins vs SQLite: {wins}/{len(samples)}")
