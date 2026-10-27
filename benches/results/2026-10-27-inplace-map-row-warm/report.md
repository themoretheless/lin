# API-preserving in-place Map Row versus qualified COW prototype

Local macOS release diagnostic; not a peer or Full-durable qualification.
Baseline: frozen COW plus bounded bitmap emptiness. Candidate: same update/undo/index mechanisms with exact public BTreeMap Row alias and no redundant effective-patch clone.
Six rotated baseline/control/candidate rounds: 18 processes, 1728 observations. Each process has 16 batches of 512 alternating updates and 16 explicit rollbacks per size. Existing row/count/gen readback assertions pass; this is not an exhaustive full-row oracle.

Positive gain means faster candidate.

| Rows | UPDATE gain % | A/A abs % | Positive pairs | Rollback gain % | A/A abs % |
|---|---:|---:|---:|---:|---:|
| 1000 | -51.359 | 0.839 | 0/6 | -22.716 | 3.950 |
| 10000 | -46.608 | 1.467 | 0/6 | -20.427 | 4.957 |
| 100000 | -48.536 | 0.642 | 0/6 | -14.376 | 7.304 |

Result: candidate materially slower than COW for this warm workload; do not claim equivalent performance. This comparison does not establish whether it improves over production main. COW remains a source-incompatible prototype because Row is frozen as a map alias. Neither variant is promoted by this report.

Correctness: ../2026-10-27-inplace-map-row/test-status.json exit 0; cargo test --lib --tests, including implicit/explicit derived-hash guard regression and compile-time exact Map alias assignment. Source restored after testing and both builds. Binary and source hashes: provenance.json. Raw data and all six rounds retained.

Next investigation: output/WAL row ownership duplication. Preserve journal byte quota, full returned rows, Done.n, transaction retention, encoding, rollback and public API; no payload elision.
