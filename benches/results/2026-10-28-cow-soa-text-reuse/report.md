# SoA unchanged-text reuse candidate

Candidate c60cb1f10 retains existing SoA Arc slots for equal SmallText strings (comparison bounded by inline capacity) and pointer-identical Text. Changed text, non-text/absent values and identity-rebuild behavior remain covered. Packed-title invalidation remains unconditional. No unbounded long-string equality comparison is added.

The same isolated allocator fixture as ../2026-10-28-cow-soa-allocation-probe/ was rebuilt with the frozen candidate. Nine fresh processes, three per short/long/mixed shape, all report zero allocation calls and zero requested bytes around 1000 unchanged-text SoA synchronization calls. Baseline short was 3000 allocations/72000 bytes; mixed was 2000/48000; long was already zero. Baseline raw files remain unchanged. Candidate source/binary hashes and the pre-commit build state are recorded in provenance.json, verified against frozen_candidate_commit.

Four focused release regressions pass: short/shared/equal-distinct shared text, title cache/grouped-layer changes, missing/nontext transitions, users/orders values and moved-identity mapping. The complete release library/integration suite passed: 453 tests, 2 ignored, 0 failures (full-tests.log and test-status.json).

This diagnostic calls Store::row_maps_sync_updated directly, not DB UPDATE. It proves reduced allocator calls in that bounded path only. It does not establish latency, RSS, durable WAL throughput, peer superiority or full-source qualification. The frozen COW+CAS Linux run 40607797376 predates this candidate and cannot qualify it. A separate paired benchmark and source gate are still required before adoption.
