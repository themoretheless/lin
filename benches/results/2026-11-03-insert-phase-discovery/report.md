# INSERT phase discovery — macOS ARM64

Diagnostic only, not a qualification or peer timing retry. Ordinary default worker workload: 1k, 10k and 100k rows, three samples each; 100k uses ten prepared 10k chunks in one memory transaction. Default embeddings, indexes and exact six-field readback remain enabled. The worker validated exact expected rows on every sample; the captured output stores result counts/digests rather than duplicating all values.

Runtime source before instrumentation: d78decdd4. `instrumentation.patch` adds diagnostic phase timers only. The built diagnostic executable is preserved locally as `/tmp/lin-insert-profile-worker-20261103`; its SHA is in discovery.json. Original exec.rs was restored byte-for-byte to HEAD immediately after build. These timings cannot be compared directly to Linux peer results and have observer overhead, no A/A control or adoption gate.

Across the 33 measured 10k chunks, embedding and maintenance dominate; row construction is substantially cheaper. `phase-summary.json` contains counts and medians, `phases.log` all phase observations. Next profile should split maintenance into secondary index / FTS / row-map costs, and embedding into text assembly / batch execution / vector attachment. No production speedup is asserted from this diagnostic.
