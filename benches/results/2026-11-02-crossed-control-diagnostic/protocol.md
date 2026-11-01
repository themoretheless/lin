# Crossed identical-binary and cross-build controls

Diagnostic following rejected balanced source run 41353987944. That result remains failed; this experiment cannot qualify COW adoption or supersede its gate.

Keep its exact BTreeMap/COW source pair, dependency inputs, complete Rust harness and ordinary workload callbacks. Rebuild serially in separate Cargo targets through the existing source-bound builder. No runtime patch, allocator change, weaker durability, fixture change or instrumentation inside timed callbacks.

Measure the three failed control workloads: DuckDB GROUP BY, SQLite DELETE 10k in DELETE-journal mode, and Lin FULL COMMIT 1k. For each workload execute six role labels: baseline_a, candidate_a, baseline_b, candidate_b, baseline_c, candidate_c. All three baseline labels use one identical executable; all three candidate labels use the other identical executable. Record and verify live hashes, exact commands and source restoration. Run all 134 full-harness functional smokes before timing.

Twelve rounds, two six-round blocks, 216 fresh processes, eight observations per process: 1728 observations. Use the frozen six-role Williams design. Every role occupies each position once per block; every ordered adjacency occurs once; each pairwise precedence occurs three times. Rotate cases separately using original case indices. Execute the prevalidated plan; reject the complete historical cancellation schedule.

For each process use the median of eight wall observations. Within each round use the median of the three baseline process medians and the median of the three candidate process medians. Report all twelve round contrasts and both six-round summaries. Also retain all three within-binary label-pair contrasts separately for both executables, with per-series median gain and positive counts. No significance or causality claim follows from a diagnostic median alone. No adoption threshold; losing, null and mixed outcomes are retained.

Archive exact Linux executables as a separate CI artifact for later machine-code inspection. Ordinary timings run without tracing. Build logs, metadata, source/input hashes, process output and numerical reconstruction remain auditable. Synthetic fixture checks are labelled synthetic and do not qualify actual engine execution.
