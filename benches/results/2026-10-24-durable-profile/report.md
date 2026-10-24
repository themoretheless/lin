# Durable write phase profile, committed source c4d223b

Measured 2026-10-24 in an isolated managed worktree pinned to c4d223b7dcd514faec7e4b9080915d4840cdc405. Primary checkout had active uncommitted embed/store/test changes; they are excluded from this profile and were not modified by this task. The primary tree continued changing independently, so this is explicitly a committed-source result, not a qualification of its dirty runtime. Recent lazy transaction journaling and official transaction cases exist in the pinned source, but their commit messages do not substitute for this bulk write diagnostic.

Temporary timers were added only to persist.rs in the isolated worktree. A separate pinned release benchmark executable was built, then persist.rs restored byte-for-byte BEFORE diagnostics. No timer code ships. Three independent processes, 24 fresh measured samples at 1k/10k plus preflight: 27 phase events per size/process, exact fixture validation outside timing, all raw run.json complete and runner exits zero. No builds/tests launched by this runner overlap timing; other host activity remains uncontrolled. Source and binary hashes, before/instrumented source snapshots, runner and summarizer retained. This diagnostic does not replace full workspace tests or compare against SQLite.

| Phase | 1k ms | 10k ms |
|---|---:|---:|
| pack_ns | 0.232333 | 2.289917 |
| classify_ns | 0.121333 | 0.378417 |
| encode_ns | 0.539416 | 1.674792 |
| checksum_ns | 0.044042 | 0.368583 |
| write_ns | 0.050250 | 0.225791 |
| sync_ns | 0.685834 | 1.447583 |

Medians of three process medians. Classify is nested inside encode; do not add it twice. Logging perturbs timing, and independent phase medians cannot be summed/subtracted to derive an exact end-to-end latency or causal improvement. Frames contain 336968 / 3558340 payload bytes at 1k / 10k. The separate frame log includes metadata frames; the summarizer associates frame metrics only with the immediately preceding typed bulk encoder event.

At 1k, sync (~0.686 ms) and encoding (~0.539 ms) dominate these WAL phases. At 10k, row-to-column packing (~2.290 ms) exceeds encoding (~1.675 ms), with sync ~1.448 ms. The next useful experiment is current row/column packing allocation/copy behavior, rather than assuming vector encoding remains the largest CPU target from the older profile. Checksum (~0.044/0.369 ms) is a secondary target; selecting another checksum algorithm requires explicit format/compatibility reasoning, not a silent speed-only substitution.

This report proves neither a performance gain nor superiority over any peer. Existing fresh native/durable SQLite gaps remain the benchmark target until remeasured on current committed/candidate variants. MSSQL/Kusto configuration and the full named-peer coverage remain outstanding. Keep default embedding, FTS, indexes and Full-sync unchanged when testing the next optimization.
