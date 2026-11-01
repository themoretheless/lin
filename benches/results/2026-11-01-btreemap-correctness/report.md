# Corrected BTreeMap reference: local qualification passed

Original6812cc5ca was compiled unchanged and reproduced all six schema rollback failures. The probe also type-checks the exact Row=BTreeMap<String,Cell> identity and records24-byte Row and24-byte BTreeMap on macOS ARM64. Full baseline source hashes, compiler output and outcomes are retained in baseline-probe/.

This reference ports the chronological append undo, HNSW delete/append restoration and terminal schema checkpoint fixes while preserving the exact public BTreeMap Row contract. UPDATE implementation remains the original whole-row BTreeMap path; COW sharing, field-delta undo, clone elision and index performance changes are excluded. Runtime differences are confined to exec.rs and store.rs; the latter also contains the three shared HNSW regression tests.

All441 default release tests/two ignored pass locally. Shared schema, mixed append/delete, HNSW and explicit/derived hash guard regressions run on this reference. The exact-type API test passes. Cargo.lock and runtime dependencies match candidate2dc10df2d. Cargo.toml only adds the same point_values target, whose source matches the candidate byte-for-byte. Its full-value oracle passes six actual Lin/SQLite/DuckDB hit/miss checks and five invalid-value/multiplicity checks. No point lookup timing qualification is claimed.

Six mutation variants have source-construction preflight only; their separately compiled runtime rejection and Linux default-feature qualification remain pending. This reference is required for a fresh whole-source comparison with the corrected COW/UPDATE candidate. It does not inherit any old component gain or peer win.
