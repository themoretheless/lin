# Bulk benchmark timing boundary

The archived full-engine run40607797376 at frozen36666994a records the same lifecycle for all72Lin bulk processes (1k/10k,12rounds,baseline/control/candidate),576wall observations:

> fresh input per operation; input setup/drop excluded; output drop included; chunks of <=64

Cargo.lock pins airbug-bench75b1321b5d7ff2cee983379f539c07726a7d27e5. Its suite_measure input_batch creates inputs before Instant::now andrecords elapsed time before input destruction. InsideTiming controls output drop; it does not include Db/input teardown. bench_checked performs validation on separate fresh operations outside timing.

The frozen Lin bulk closure invokes fill_lin,which runs the prepared statement andextracts done.n. The temporary execution Handle is destroyed inside fill_lin;theclosure output is usize. Input Db andPrepared destruction remain outside the timer. Bulk setup enables the same default embedder,scalar index andFTS before preparing the insertion.

Therefore profiling Db::drop cannot explain a regression in the recorded bulk timing. A diagnostic must isolate prepared execution andits temporary result/transaction lifetime. It must keep preparation,initial database/index setup,validation andinput teardown outside its measured region. No existing timing observations oracceptance thresholds are changed by this clarification.

The current executor also already skips rows_to_insert_cols for a non-durable insert andmoves large batches into Store while eliding Handle.rows above128rows. Those existing optimizations must not be proposed as new changes. Smaller Row-descriptor traffic orenum dispatch remains an unproven cause of the5–8%bulk regression;the rejected owned-map-template experiment did not establish it.

bulk-lifecycle-contract-audit.json records all72raw hashes. This is a contract audit,not a new performance result oracceptance of any COW candidate.
