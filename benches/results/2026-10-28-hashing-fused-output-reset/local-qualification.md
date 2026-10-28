# Functional qualification

12focused embedding tests and450release library/integration tests passed,2ignored. Existing dense-feature bit oracle and occupancy/dimension/large-normalization/reset/held-output regression retained from2c8515692. Three separate mutants failed at runtime: missing copied coordinate, stale scratch value and stale seen bit. Every candidate source hash restored; logs and exact mutant sources retained.

Source-shape proof confirms that only output materialization/reset region differs from preceding rejected candidate2c8515692; normalization, features and tests are byte-identical outside that region. This is not a timing or incremental speedup claim. The unchanged baselinecomparison and8controls remain in protocol.md. Complete source/engine and eight-peer qualification still required.

Safety invariant: every output element initialized before assume_init; normalized touched value copied before its scratch slot is reset. Dense path copies full vector before reset. Scratch clearing does not alias any retained Arc output. No new API methods beyond the preceding Rust1.85 standalone compatibility proof.
