# Owned-map bulk template component

Candidate a94bab306 versus preceding COW+CAS a7d4b02c3; only src/exec.rs differs. The later SoA candidate is excluded. Same ordinary release compare harness, payload/embedding/validation, runtime, schema and sync policies. Current full-source COW+CAS rejection remains valid; this component cannot qualify the entire engine.

Two targets: native Lin bulk INSERT1k/10k. Six controls: Lin filter_eq,prepared UPDATE1k/100k,scalar Full/Normal WAL,and SQLite bulk INSERT10k. Existing cases and their setup/teardown timing contracts remain unchanged. Two six-pair series with rotated case/role order, baseline/control use the same executable, eight observations each, max_iterations1,zero warmup. All288fresh processes/2304observations retained; no startup trimming. Each target window needs6/6positive pairs and median gain above median absolute A/A. Every control needs gain>=-max(2%,A/A).

449local release tests/2ignored passed, including10focused reference-construction regressions. Linux reruns the complete suite and16fresh correctness smokes before timing. Synthetic matrix checks are explicitly not benchmark results. No production adoption,full-source acceptance,memory claim or eight-peer victory follows from component success. Preserve failed pairs and complete provenance; do not rerun unchanged to obtain a pass.
