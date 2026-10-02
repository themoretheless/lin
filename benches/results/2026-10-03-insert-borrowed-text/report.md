# Borrow row text during bulk insertion — rejected

Removed three temporary Arc reference-count increments/decrements per inserted row: body is borrowed for content hashing; id and uri Arc values are borrowed directly and cloned only for the batch uniqueness sets. Public APIs and source values unchanged. Candidate builds; all 61 exec tests pass, including uniqueness atomicity, hybrid bounds and bulk result elision.

Six alternating process pairs for native and six for durable insert; 24 fresh fixtures per case, one operation. Default embedding, FTS, scalar indexes and durable WAL sync_data preserved. Exact inserted common values validated outside timing. No builds/tests overlapped timing. Fixed case order and uncontrolled host load; no causal significance claimed. Process medians first, paired relative decreases then summarized. Milliseconds below are median of process medians.

| Path | Rows | Lin baseline | Candidate | Paired decrease | Wins | SQLite control decrease |
|---|---:|---:|---:|---:|---:|---:|
| native | 1k | 1.308240 | 1.290448 | 1.54% | 4/6 | -0.11% |
| native | 10k | 13.591563 | 13.422291 | 1.27% | 4/6 | 0.59% |
| durable | 1k | 3.300729 | 3.422531 | -3.26% | 1/6 | 2.78% |
| durable | 10k | 26.206489 | 26.083198 | 0.49% | 5/6 | 0.17% |

Reject: native gains are small (4/6 wins at both sizes), while target durable 1k loses 5/6 pairs with a -3.26% median decrease; unchanged SQLite durable 1k controls improve +2.78%. This does not prove a causal mechanism, but does not justify retention as a performance improvement. Baseline src/exec.rs restored byte-for-byte after saving candidate source and patch. MySQL benchmark changes from the preceding turn preserved. No all-peer victory claimed.
