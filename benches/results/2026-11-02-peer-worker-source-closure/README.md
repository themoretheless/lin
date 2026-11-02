# Peer worker input closure

examples/peer_bench.rs is an include! compatibility wrapper. The actual Rust protocol, data adapters and timers are in tools/peer-worker/src/main.rs. Future source snapshots now include that source tree and the worker Cargo/build manifests, along with existing runtime, derive, Python driver and explicit executable identities.

Seven source-integrity tests and seventeen harness contract tests pass, including edits to worker source/manifests. This branch also carries the already-validated peer-only snapshot fix from the corrected Ubuntu protocol, so a peer-only invocation does not require a Lin binary while Lin invocations still fail closed on missing executables.

Git blob comparison confirms the wrapper, actual worker and worker Cargo manifest are byte-identical between frozen Ubuntu runs 34ce688a3 and 10e189548. The complete source of the latter worker is retained here. Checks also record availability/hashes at runtime 625, shared-key experiment 62a242111 and WAL experiment 20a408f6c.

The historical peer fingerprint lists did not directly cover the included worker source. This is a recorded provenance limitation; checking current Git blobs is not a retroactive before/after runtime fingerprint. Original raw observations, source_drift flags, failing win gate and frozen manifests remain unchanged. No unchanged benchmark rerun or all-peer superiority claim is made by this source-closure correction.
