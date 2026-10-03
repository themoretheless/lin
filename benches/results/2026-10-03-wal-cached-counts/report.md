# Rejected sparse WAL cached-count experiment

Cached each row's nonzero count while choosing sparse codec, then reused counts
while serializing. This removed a vector scan but added one temporary Vec<usize>
per vector column. Frame representation, sparse selection thresholds, decoded
bounds and Full sync were unchanged. Persistence bit-preservation/checksum and
truncation unit tests passed (6). Baseline/candidate benchmark builds passed.

Six alternating independent process pairs, 24 fresh fixtures per case, one
operation per sample, no warmup. Setup/preparation/validation/drop excluded;
existing readback/count gates passed. No builds/tests overlapped timing. Host
load and filesystem scheduling are uncontrolled. Aggregate process medians;
paired gain is the median of six per-pair relative changes.

| Durable case | Baseline ms | Candidate ms | Paired gain | Wins |
|---|---:|---:|---:|---:|
| Lin 1k | 3.233135 | 3.448479 | -5.14% | 1/6 |
| Lin 10k | 25.070302 | 24.401886 | 3.38% | 4/6 |
| SQLite 1k control | 1.785250 | 1.773094 | 0.46% | 4/6 |
| SQLite 10k control | 18.052771 | 18.040709 | 0.43% | 5/6 |

Rejected: small durable insertion regressed substantially with a comparatively
stable SQLite control. The larger-batch gain was less consistent than the
retained zero-block optimization. These wall-clock results do not establish
the mechanism of the small-batch regression. Production persist.rs restored
byte-for-byte to 9462842, retaining the previously validated zero-block path.
Raw observations, source/binary SHA256 and candidate source are retained.
Trailing blank log lines normalized; raw JSON observations unchanged.

The full eight-engine goal remains unproven: SQLite insertion still wins and
MSSQL/Kusto test endpoints are unavailable. Removing repeated vector scans alone
has not yielded a stable small-batch improvement; broader insertion cost or
allocation changes need independent evidence before retention.
