# Normal-mode five-write transaction diagnostic

Three fresh processes per engine,1000 growing transactions with five inserts each. Dynamic Lin run and SQLite execute parse inside timing, following the old test. Per-transaction clocks and Lin counter reads outside timing make this a new diagnostic contract, not a replication of its aggregate timer. Default Lin checkpoint every32 commits and SQLite WAL autocheckpoint1000 pages are unchanged. All supplied values and multiplicity checked after ordinary close/reopen.

| Repeat | Engine | Mean ns/tx | Median ns/tx | Checkpoints | Checkpoint mean ns | Regular mean ns | Checkpoint share of timed total % |
|---:|---|---:|---:|---:|---:|---:|---:|
| 1 | lin | 637869.4 | 37583.0 | 31 | 19148224.5 | 45690.9 | 93.06 |
| 1 | sqlite | 20652.8 | 15895.5 | not observed | 0.0 | 0.0 | 0.00 |
| 2 | sqlite | 18585.2 | 15208.5 | not observed | 0.0 | 0.0 | 0.00 |
| 2 | lin | 583309.8 | 18625.0 | 31 | 17817401.8 | 31961.2 | 94.69 |
| 3 | lin | 643594.6 | 41417.0 | 31 | 19121490.5 | 52454.5 | 92.10 |
| 3 | sqlite | 21607.4 | 17645.5 | not observed | 0.0 | 0.0 | 0.00 |

Checkpoint-labelled samples include the whole transaction, not isolated checkpoint spans. SQLite checkpoints are not independently instrumented. Counter reads and clock overhead are not calibrated away. No candidate/A/A causal speed claim, no Linux inference from macOS and no durability-policy recommendation follows from this diagnostic. End-of-run Normal close/checkpoint is outside the operation timer and occurs before owned fixture cleanup. Normal mode does not promise each transaction survives power loss.

Frozen source: `d2ec61fee52c57272eba6810493de33e557985a7`; current production Row alias preserved. Raw six process outputs, source/lock/binary hashes and build log retained.
