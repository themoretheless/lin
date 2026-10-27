# Physical WAL zero-tail preallocation: naive approach rejected

Actual unchanged production reader/core, pinned by provenance.json. open_log uses read/write without O_APPEND. Ordinary writes advance the file cursor; recovery truncates to the last valid offset, and Persist.log_bytes is initialized from physical file length. Merely extending file size therefore also requires careful logical-byte quota/stats handling. Existing KEEP_SIZE work is a different mechanism: it does not change EOF and remains separately rejected on its performance gate.

The standalone release probe creates two real canonical codec-1 checksummed records with Full sync (3947 bytes each). It retains the first complete frame plus half of the second, then compares unchanged-reader recovery:

| Fixture | Physical bytes | Replayed generations | Result |
|---|---:|---|---|
| Ordinary torn EOF | 5920 | 1 | success, end3947 |
| Same prefix padded with zeros | 1048576 | 1 | corrupt log record |
| Both complete frames padded with zeros | 1048576 | 1,2 | success, end7894 |

Thus a successful read of a complete padded WAL does not prove crash compatibility. Padding makes the partial record appear physically complete; CRC detects missing bytes as corruption instead of EOF truncation. This fixture models an interrupted unacknowledged append, not a physical power-cut test. No writer, frame format, EOF handling or corruption policy was changed. The probe itself passes by asserting this incompatibility. An initial Arc<str>/String compile error is preserved separately and is not runtime evidence.

A real preallocated writer would need an explicitly qualified logical-end/commit-boundary protocol, writable/read-only/legacy recovery, export/apply, checkpoint, offsets and logical quotas. Ignoring arbitrary CRC errors at the tail would weaken existing corruption detection and is not proposed. No performance run or production adoption is justified for the naive set_len scheme. Reproduce with `python3 benches/results/2026-10-27-wal-physical-tail-feasibility/run.py` from the repository root.
