# Linux rollback correctness qualification

Run [40648113616](https://github.com/themoretheless/lin/actions/runs/40648113616), frozen source `21dcd7d81354aa2f59038aa6cb142c86cd1cd1f5`, completed successfully. Ubuntu x86_64, Rust1.96.0: **454 release tests passed, 2 ignored**.

Three independently compiled runtime negatives were rejected by the expected regression: missing restored DELETE HNSW node, stale appended HNSW node, and the old unordered append journal. All frozen inputs were restored after the negative variants.

Independent audit checked the frozen Git source/manifest, compiler/platform, complete test totals, exact mutant source and actual runtime assertion failures. Eight corrupted raw packages were rejected, covering source identity, counts, restoration, platform, mutant source, missing runtime failure, unrelated failing test and missing full-suite result.

All raw files are retained in raw.tar.gz; every archive member and the archive itself have SHA256 records. Unpacked copies remain locally for inspection. This is a correctness prerequisite, not a latency/RSS result, production adoption, full-source performance gate or completion of the eight-peer goal. Historical timings do not qualify the corrected source.
