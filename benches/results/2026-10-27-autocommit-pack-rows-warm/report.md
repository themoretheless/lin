# Recover autocommit output rows from WAL Pack

API-preserving prototype layered on in-place Map Row + delta undo + bounded bitmap prefix. Baseline is the same Map prototype without Pack recovery, not production main and not COW. Single-statement nontransactional UPDATE/DELETE defer returned rows until successful commit, then move full rows out of Pack. Store retains its existing public durable_commit API; internal recover helper returns the original owned payload after append. Explicit transactions and multi-statement programs keep original pack retention/cloning and journal quotas. No fields, encoding, FTS, embeddings or output payloads removed.

Correctness: full cargo test --lib --tests passes. Added native/Full durable update/delete exact full-output equality and reopen survivor check pass; zero matches retain runtime error. The added test is reopen qualification, not an independent crash/replay test. Existing integration persistence tests ran in full suite. An intentional missing-output-recovery mutant fails full-row equality (101). Initial test assumed zero-match succeeds; failed log preserved and oracle corrected to existing contract. Source restored after every test/build.

Local macOS release warm diagnostic: six rotated A/A/B rounds, 18 processes, 1728 observations. Each process has 16 batches of 512 alternating prepared updates and 16 explicit rollbacks per size. Readback checks fields, count and generation; separate full-payload oracle above.

| Rows | UPDATE gain % | A/A abs % | Positive | Rollback gain % | A/A abs % |
|---|---:|---:|---:|---:|---:|
| 1000 | 16.866 | 1.072 | 6/6 | -4.759 | 3.814 |
| 10000 | 17.036 | 0.846 | 6/6 | -2.650 | 1.646 |
| 100000 | 17.259 | 1.805 | 6/6 | -1.338 | 1.663 |

UPDATE gain is consistent in this diagnostic. Rollback regresses beyond A/A at 1k and 10k; cause not established, despite its logical mutation path remaining unchanged. No general acceptance, no peer victory, no production promotion. Need investigate unaffected-path cost/code generation and fresh isolated native/durable controls before selecting this mechanism. Do not rerun unchanged seeking green. Raw observations, process status, binary/source hashes retained.
