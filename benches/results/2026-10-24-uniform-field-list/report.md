# Uniform schema first-key list: rejected

Isolated committed baseline c4d223b7dcd514faec7e4b9080915d4840cdc405, production identical to the previous phase profile. Primary checkout has concurrent uncommitted changes and is explicitly excluded from both binaries. Candidate caches first-row keys in a Vec<&str>, compares later rows against the contiguous list, and lazily constructs the union BTreeSet only for a schema mismatch. Sorted field order, first-non-null inference, value coercions and row-major packing remain unchanged. Native in-memory skips this pack path, so native shifts must not be interpreted as a direct algorithm gain.

Four focused tests pass: 1001 uniform Unicode/SSO/text rows preserve names/values; same-width different-key rows use full union; NULL-first with late extra field preserves fallback defaults; an empty initial row does not hide later fields. Both release builds succeed. Six alternating process pairs per native/Full durable mode; 24 fresh fixtures per case, one operation per sample, no warmup; exact existing fixture validation outside timing. Same unchanged SQLite implementation is measured in each binary. Binary/source hashes and raw reports retained. No builds/tests initiated by this runner overlap timing; unrelated host activity remains uncontrolled. Positive paired gain means faster; wins count process medians.

| Case | Median paired gain | Wins |
|---|---:|---:|
| compare/insert_bulk_1k/lin | -1.007% | 2/6 |
| compare/insert_bulk_1k/sqlite | +0.748% | 3/6 |
| compare/insert_bulk_10k/lin | +1.269% | 4/6 |
| compare/insert_bulk_10k/sqlite | +0.515% | 5/6 |
| compare/durable_insert_1k/lin | +3.929% | 4/6 |
| compare/durable_insert_1k/sqlite | +29.232% | 4/6 |
| compare/durable_insert_10k/lin | -0.306% | 3/6 |
| compare/durable_insert_10k/sqlite | -4.604% | 1/6 |

Reject: target durable 10k is flat/slightly slower with 3/6 wins. The apparent 1k gain has a much larger 29.2% SQLite control improvement; this is insufficient evidence for retention or a causal speedup. Do not normalize away this control shift or report an isolated gain. No independent repeat or full workspace run was required for this rejected prototype. persist.rs restored byte-for-byte in the isolated checkout; no production DB changes ship. Before/candidate snapshots preserve code and focused tests.

The prior profile's packing cost remains an open target. This result narrows the next hypothesis toward per-value copying/allocation rather than first-key traversal, but does not directly attribute cost to either mechanism. Full named-peer objective remains active, including current committed/candidate verification, SQLite gaps, unconfigured MSSQL/Kusto and honest cross-engine feature/durability scope.

Text logs have trailing blank lines normalized; run.json observations and source snapshots are unchanged.
