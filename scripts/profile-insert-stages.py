"""Record transaction begin, ten prepared inserts, and commit separately."""
import json
import os
import subprocess
import sys

request = json.dumps(dict(insert_rows=100000, samples=3, timestamp_ms=123456))
results = []
for trial in range(8):
    for mode in (("default", "thp") if trial % 2 == 0 else ("thp", "default")):
        env = dict(os.environ, LIN_BENCH_INSERT_STAGES="1", LIN_BENCH_INSERT_SMAPS="1")
        env.pop("GLIBC_TUNABLES", None)
        if mode == "thp":
            env["GLIBC_TUNABLES"] = "glibc.malloc.hugetlb=1"
        run = subprocess.run([sys.argv[1]], text=True, input=request, env=env, capture_output=True, check=True)
        data = json.loads(run.stdout)
        components = {"build": [], "reserve": [], "minor_faults": [], "shape_setup_sum": [], "sampled_vector_ns": [], "sampled_cells_ns": [], "sampled_rows": [], "fill_ns": [], "copy_ns": [], "alloc_ns": [], "write_ns": [], "validate": [], "maintain": [], "append": [], "maint_setup": [], "maint_parallel": [], "maint_fts": [], "maint_index": [], "maint_maps_hnsw": []}
        pages = {phase: {field: [] for field in ("Anonymous", "AnonHugePages")} for phase in ("before", "after")}
        for line in run.stderr.splitlines():
            if line.startswith("insert_pages "):
                _, phase, field, kib, unit = line.split()
                assert unit == "kB"
                pages[phase][field.rstrip(":")].append(int(kib))
            if line.startswith("insert_profile "):
                _, component, ns = line.split()
                components[component].append(int(ns))
        for phase in pages.values():
            assert len(phase["Anonymous"]) == len(phase["AnonHugePages"]) == 3
            assert all(0 <= huge <= anon for huge, anon in zip(phase["AnonHugePages"], phase["Anonymous"]))
        assert all(len(values) == 30 for values in components.values())
        assert min(components["fill_ns"]) > 0 and min(components["copy_ns"]) > 0
        assert all(a + w == c for a, w, c in zip(components["alloc_ns"], components["write_ns"], components["copy_ns"]))
        sampled_rows = components.pop("sampled_rows")
        assert sampled_rows == [625] * 30
        assert len(data["result"]) == 100000
        ends = data["stage_end_ns"]
        assert len(ends) == 3 and all(len(sample) == 12 for sample in ends)
        durations = [[b-a for a,b in zip([0]+sample, sample)] for sample in ends]
        assert all(min(sample) >= 0 for sample in durations)
        results.append(dict(trial=trial, mode=mode, pages_kib=pages, stage_ns=durations, sampled_rows=sampled_rows, minor_faults=components.pop("minor_faults"), component_ns=components, total_ns=data["samples_ns"]))
print(json.dumps(results))
