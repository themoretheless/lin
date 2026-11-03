# Linux INSERT phase attribution protocol

This diagnostic profiles the unchanged ordinary f37 runtime (current branch source equivalent), not either rejected ASCII/grouped-index candidate. Ubuntu 22.04 native x86_64, Rust 1.96.0, locked ordinary default-feature release worker. Builds two executables from one source closure: ordinary, then the two preserved diagnostic patches combined. Original source is restored and rehashed before measurement, including on build failure. The exact two ELF files, source/patch hashes and build logs are artifacts.

Six fresh worker processes: ordinary and diagnostic for 1k/10k/100k, three samples each. The existing worker uses prepared statements, keeps embeddings/FTS/index maintenance enabled, and chunks 100k into ten 10k statements in one memory transaction. Full exact six-field expected values are independently checked outside timing and retained, along with all wall samples and phase observations.

Attribution covers row building, validation, embedding text/batch/attachment, reservation, secondary indexes, FTS, HNSW and row maps. The residual between wall and non-overlapping coarse phase totals includes remaining transaction/statement/append work and timer-output overhead; it is not pure transaction cost. Ordinary/diagnostic timings are not a candidate speedup comparison. No peer benchmark is rerun and no adoption gate can pass from this diagnostic. All eight peer requirements and prior numerical failures remain binding.

Preflight verified both patches compose, all phase markers occur once, Python parses, and original exec.rs is restored byte-for-byte. Results are pending CI; no Linux phase timing is claimed yet.
