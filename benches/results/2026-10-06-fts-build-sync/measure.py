"""Run two lib-test binaries in alternating fresh processes; timings are FTS-only."""
import hashlib, json, pathlib, re, statistics, subprocess, sys
out = pathlib.Path(__file__).resolve().parent
binaries = dict(zip(["baseline", "candidate"], sys.argv[1:]))
assert len(binaries) == 2, "pass baseline and candidate test binary paths"
runs = []
for pair in range(7):
    for variant in (["baseline", "candidate"] if pair % 2 == 0 else ["candidate", "baseline"]):
        p = subprocess.run([binaries[variant], "fts::tests::optimization_measurement", "--exact", "--ignored", "--nocapture"], capture_output=True, text=True, check=True)
        values = {name: int(ns) for name, ns in re.findall(r"^((?:build|sync) .*): (\d+) ns$", p.stdout, re.M)}
        assert len(values) == 6
        runs.append(dict(pair=pair, variant=variant, values=values, output=p.stdout))
medians = {variant: {name: statistics.median(r["values"][name] for r in runs if r["variant"] == variant) for name in runs[0]["values"]} for variant in binaries}
result = dict(binary_sha256={v: hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest() for v,p in binaries.items()}, runs=runs, medians_ns=medians, speedup={name: medians["baseline"][name]/medians["candidate"][name] for name in medians["baseline"]})
(out/"run.json").write_text(json.dumps(result, indent=2))
print(json.dumps({"medians_ns":medians,"speedup":result["speedup"]}, indent=2))
