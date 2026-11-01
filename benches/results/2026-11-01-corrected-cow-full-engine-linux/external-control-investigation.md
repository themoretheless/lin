# External control investigation: no cause established

The eight failed external windows remain binding and the source gate remains rejected. No new timing was run.

The frozen compare harness registers Lin/SQLite/DuckDB cases through lazy Fixture::new. Airbug revision75b1321b stores the initialization closure and initializes its OnceCell only when Fixture::get is called. bench_fixture calls get inside the selected case runner before starting its timer; suite selection filters entries before executing them. Therefore the proposed explanation that every external process first fills every Lin table is not supported by the actual code. Registration allocations and common executable code still exist, but their causal effect is unmeasured.

The external engine case bodies are unchanged across the two role-specific executables. This is not proof that their compiled paths, scheduling, CPU frequency or memory state are identical. The executable payloads were not archived, so post-hoc machine-code comparison cannot be reconstructed from this artifact. Recorded role hashes, per-process resources and all negative windows remain available. Peak RSS values in external-control-investigation.json are observations, not a causal explanation.

A future investigation needs a predeclared crossed comparison of identical-executable A/A and cross-build B/C external cases, or a separately frozen external worker. It must retain null-control failures and must not retroactively change acceptance of run41306613457. No such new timing diagnostic is dispatched by this note.
