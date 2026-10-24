# Bulk Row template clone versus sorted collect — CPU gate

2026-10-24 baseline 66fb12e. Current production append_batch_shape_rows, record_row and value_cell copied into standalone diagnostic, using public Row/Value and a local Record with identical fields. Candidate keeps per-batch shape detection, last-duplicate source slots, 128-row/32-field gates and heterogeneous fallback; replaces template.clone +values_mut assignment with template.keys zipped with source slots mapped to final cells then collect::<Row>(). No prepare-time AST sorting/mutation. This differs from the old rejected prepared-sorted-records proposal and compares against the currently accepted batch template.

Both paths preserve public BTreeMap<String,Cell> Row, key order, input records, duplicate last-wins and supplied-now semantics. Independent scalar record_row is the reference. Finite preflight: batch lengths0/1/127/128/129/4095/4096, input field counts0/1/7/32/33 with duplicate keys, Now/Int values, two supplied times, existing output prefix and input immutability; default text/time fixtures and heterogeneous reversed/duplicate-title shapes. This is not exhaustive all-Value, threading or whole-transaction qualification.

Two series ×six independent processes, 24 rotating samples per variant/case/process. Outer result capacity allocation, per-batch template/slots, every Row construction/key copy, output black_box and destruction included. Input creation/validation outside timing. Source/binary hashes, compiler, helper/build/runner and raw samples retained. No runner builds/tests overlap timing; unrelated host activity uncontrolled. No full-engine, parallel row-build or peak-RSS claim.

| Series | Fixture | Template clone us | Sorted collect us | Median paired gain | Positive processes |
|---:|---|---:|---:|---:|---:|
| 1 | default_1k | 204.052 | 290.961 | -42.720% | 0/6 |
| 1 | default_10k | 2052.396 | 2918.805 | -42.818% | 0/6 |
| 1 | heterogeneous_1k | 278.046 | 280.252 | -0.389% | 1/6 |
| 1 | small_127 | 34.698 | 34.670 | -0.159% | 1/6 |
| 2 | default_1k | 203.955 | 290.058 | -42.622% | 0/6 |
| 2 | default_10k | 2055.880 | 2914.810 | -42.473% | 0/6 |
| 2 | heterogeneous_1k | 279.373 | 280.971 | -0.570% | 1/6 |
| 2 | small_127 | 34.439 | 34.522 | -0.237% | 2/6 |

## Decision: reject

Uniform default1k/10k about42% slower in both series, 0/6 positive pairs. Heterogeneous and small fallback cases near flat/slightly negative. No allocation tracing/assembly attribution was performed, so do not assign this slowdown to a specific stdlib internal mechanism. Sorted keys alone did not improve current row construction. Current template path is retained byte-for-byte; no additional engine gate warranted for unchanged losing candidate.

The initial source audit also verified append-only transactions use length marks rather than full row backup, large inserted rows move into the store, and durable vector columns already share Arc backing. This audit found no redundant full-row rollback clone to remove in the current default insertion path. These are source observations, not a performance benchmark.

Production source unchanged and all core hashes verified after run. No workspace rerun for diagnostic-only code, no peer win inferred. Further row-layout work must preserve public Row compatibility or define a separately justified API design; this report does not authorize a breaking key representation change. Full eight-peer goal remains open, including durable SQLite gap and absent live MSSQL/Kusto qualification.
