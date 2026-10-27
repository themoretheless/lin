# Incremental owned rollback: allocation requests reduced

Exact candidate engine bytes match the warm diagnostic and frozen Linux input. Separate release baseline/candidate executables use a System-delegating GlobalAlloc observer. A constant-initialized thread-local flag enables counters only on the operation owner thread. Successful alloc/alloc_zeroed, realloc and dealloc are counted separately, including requested layout bytes; failed allocations are refused. Counter atomics and observer branches perturb runtime, so no timing is measured or inferred. This is not allocator usable-size, retained heap, RSS or energy measurement.

Three fresh processes per variant,1k/10k/100k default embedding/FTS/index fixtures,6samples each for a two-UPDATE control window and a one-transaction UPDATE+rollback window:216windows. Fixture/prepared-query construction, exact canonical full-row serialization/index-count/generation readback and JSON output stay outside observation. Returned Handle/undo/Pack/drop and rollback are inside. No shared Row owner is retained by the serialized expected output. All operations and full readback pass; source bytes restore after build.

| Rows | Window | Baseline allocations | Candidate allocations | Baseline requested bytes | Candidate requested bytes |
|---:|---|---:|---:|---:|---:|
| 1000 | update_pair | 141 | 141 | 9848 | 9848 |
| 1000 | rollback | 154–158 | 72 | 9922–11130 | 5356 |
| 10000 | update_pair | 141 | 141 | 9848 | 9848 |
| 10000 | rollback | 154–158 | 72 | 9922–11130 | 5356 |
| 100000 | update_pair | 141 | 141 | 9848 | 9848 |
| 100000 | rollback | 154–158 | 72 | 9922–11130 | 5356 |

All54UPDATE-pair comparisons have identical counters, including frees/reallocs. All54rollback comparisons save82–86allocation requests and4566–5774requested bytes per complete UPDATE+rollback. Baseline first-window FTS pending-container allocations can remain owned by the Db beyond the observation; raw free-byte counts retain this distinction. Later windows have different pending-container state and are not dropped or relabelled as identical cold measurements. Candidate counters are stable in all samples. No allocation-breakdown mechanism is attributed to individual edits without an isolated intervention.

`verify.py` independently validates raw JSONL/combined coordinates, process identities, immutable baseline and candidate source hashes and every comparison. Full engine Linux source/peer/RSS qualification remains pending; this artifact does not establish it.
