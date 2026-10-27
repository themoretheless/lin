# End-to-end I/O hypothesis: bounded positive-zero MessagePack view

This experiment asks whether fewer WAL bytes improve complete prepared operations despite a known CPU-component tradeoff. It does not relabel the failed CPU gate: bounded encoding improved default UPDATE/DELETE serialization about20-22% in its CPU harness,but empty/no-vector serialization cost about5 ns more and failed its primitive2% control threshold. All three primitive revisions and losses are preserved separately. CPU component acceptance remains false in this experiment's qualification metadata.

Baseline is the corrected core source at33c52f09e (transaction atomic/generation,abort and stale-HEAD recovery fixes). Candidate changes only src/persist.rs to the bounded persistence-only serializer view. Logical full Rows/Packs,public Row BTreeMap alias,public serde,quota accounting,canonical16MiB frame rejection boundary,CRC/frame codec1,checkpoint interval and sync modes are unchanged. Existing reader accepts the alternate MessagePack integer representation of exact positive f32 zero. Negative zero and all other values retain their original representation. No fields or vectors are dropped. A first-row/first4-cells/first8-values bounded hint selects compaction;misses fall back to canonical encoding. Nested first Batch selection is capped at2 levels.

A real Doc20 UPDATE/DELETE frame shrinks4125->1169 bytes with739 exact positive zeros and full legacy decode equality. This is a wire-size result,not latency proof. Candidate passes435 local default release tests /2 ignored. A deliberate abs()-based negative-zero normalization mutant must fail the bit-exact legacy oracle. JSON diagnostics use a single write to avoid parallel-test output interleaving;the pre-fix diagnostic log is retained and was not parsed as valid evidence.

## Workloads and unchanged global gate

Retain every previous61 case:24 Full durable mutation/rollback cases with both SQLite FULL DELETE and WAL;37 native mutation,transaction,bulk/durable insertion and COUNT read controls. Add two real typed scalar UPDATE controls (1000 rows,Full and Normal,no vector column declared). Their complete supplied-field readback and no-vector shape are checked after ordinary close/reopen. These new controls do not disable embeddings/FTS in existing Doc fixtures.

12 rounds in two6-round series,63 cases,baseline/control/candidate roles=2268 isolated processes and18144 observations. Original/control share one uninstrumented executable;candidate uses a separately built executable.8 observations,one iteration,no warmup,fresh fixture,existing DropPolicy::OutsideTiming lifecycle. Rotate case/role order. No preload,allocator tuning,sync relaxation or loss removal. Whole-process RSS includes setup/oracle/reopen/teardown.

Six targets remain Lin Full durable UPDATE/DELETE/commit at1k/10k. Each needs6/6 positive process pairs and median gain greater than median absolute A/A in each series. All57 other cases require gain >= -max(2%,A/A) in each series. These end-to-end thresholds are unchanged;the two scalar controls broaden the scope. A passing whole-engine latency gate would not retroactively pass the CPU primitive gate or establish all-eight-peer superiority. Production adoption is not automatic.

## Qualification and provenance

CI verifies frozen source/build/helper hashes,synthetic analysis sensitivity,full435-test functional suite,negative-zero runtime mutant,two uninstrumented builds and126 correctness smoke cases before timing. Independent verify.py recomputes all raw medians,pairs,RSS,peer wins and gate decisions. Synthetic data are not engine performance evidence. Upload artifacts even on failure/cancellation;incomplete matrices cannot qualify.

The120-minute job budget covers full build/testing and the expanded timing matrix. MSSQL/Kusto/pandas/PostgreSQL/MySQL/MongoDB results are outside this isolated Rust matrix;the available-peer API refresh is separate. Full eight-peer objective stays false.
