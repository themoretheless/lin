# Generic grouped SUM prototype — target coverage failure

Candidate `ed5d052a9`, baseline production `0dab1a9a4`. Full local workspace passed 408 tests/3ignored; generic quota-boundary, float-order and owned-key regressions passed. Complete108-process/648-case/15552-sample matrix and all values independently audited and retained.

However, the orders benchmark queries select an unchanged SoA fast path and never execute the modified generic SUM loop. The audit confirms that this fast-path source block is byte-identical in both builds. The numerical gate also fails, but these measurements cannot establish this prototype's performance. The experiment is classified as misdirected, not as proof that borrowing generic keys is slower. Its source was restored before the separate actual-path correctness repair. No runtime adoption, unchanged retry or discarded observations.
