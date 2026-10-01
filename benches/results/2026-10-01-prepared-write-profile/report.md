# Prepared write diagnostic profile

Command: `cargo run --release --offline --example profile_writes -- --rows 10000 --seconds 25`.
After PROFILE_READY, macOS `sample PID 5 1` recorded five seconds of stacks with a 1 ms interval.
The hot loop rotates through 10000 precompiled delete/insert pairs, keeping ID, URI,
title, body and wing; timestamps are evaluated at insertion. Seed/prepare and final
readback are outside the sampled loop. Deletes need CAS; the body hash stays fixed.

The run completed 3131541 pairs in 25.001 seconds and validated final row count and
every ID/URI lookup. This is a diagnostic churn workload, not a peer benchmark or
a measurement of fresh-fixture single-delete latency. Do not equate its pair rate
with native compare.rs delete throughput.

Among 4142 main-thread sampled stacks, top-of-stack counts include union_sorted 255,
subtract_sorted 143, FtsIndex::move_row 110 and FtsIndex::remove_row 85. These are
sampling observations, not exact elapsed-time percentages or allocation counts.
They justify investigating pending FTS folding and posting updates next. Repeated
churn amplifies delta folding; fresh one-row native mutations require their own A/B
measurements before retaining a change.

Full raw symbolized sample and source hashes are saved beside this report.
