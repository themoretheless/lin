# SoA text reuse component protocol

Candidate c60cb1f10 versus preceding COW+CAS a7d4b02c3, only src/store.rs differs. Same frozen compare.rs, Rust toolchain, ordinary uninstrumented release builds, fixture bytes and sync policies. This component does not replace the pending full source/peer gate.

Nine cases: three native prepared single-row UPDATE targets at 1k/10k/100k; scalar Full/Normal WAL, native DELETE1k, rollback1k/100k and SQLite UPDATE1k controls. Two six-pair series, rotating cases and baseline/control/candidate roles. Baseline and control use one executable. Eight wall observations, max_iterations1, zero warmup, no trimming. 324 fresh processes/2592 observations retained. Every target window needs six positive pairs and median gain above median absolute A/A; controls need gain >= -max(2%,A/A). No gate relaxation, unchanged retry, allocation-as-throughput claim or peer-completeness claim.

453 local release tests passed, 2 ignored, including four SoA regressions. CI independently reruns the full suite and performs all18 one-sample correctness smokes before timing. Setup/readback/reopen validation and teardown follow the unchanged benchmark harness. Timing includes no extra allocator instrumentation. Linux environment and all failing observations must be retained. Full-engine qualification and the eight-peer objective remain false regardless of component outcome.
