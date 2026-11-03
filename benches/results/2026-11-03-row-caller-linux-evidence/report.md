# Caller-built final bulk chunk: rejected

Run https://github.com/themoretheless/lin/actions/runs/41626345883 atc0979a4aad92630b4231397d3c6935b5efc321ad completed216processes/6156operations/108lifecycle-wall/108RSS. Independent audit verified complete source/harness/helper closure, actual lin/worker compilation for both ELF binaries, workspace correctness, exact result readback/digests and every numerical window. Local workspace415passed/0failed/3ignored. Numerical rejection, not infrastructure failure.

| Native rows | Series | Median gain % | A/A % | Positive pairs | Passed |
|---|---|---:|---:|---|---|
| 1000 | 1 | 1.149234 | 2.128406 | 4/6 | False |
| 1000 | 2 | 7.355493 | 12.715546 | 5/6 | False |
| 10000 | 1 | 0.625862 | 3.541156 | 3/6 | False |
| 10000 | 2 | -3.390025 | 5.680221 | 2/6 | False |
| 100000 | 1 | 0.588779 | 1.299355 | 5/6 | False |
| 100000 | 2 | 0.385910 | 2.665608 | 3/6 | False |

Zero of six native target windows passed; all twelve read controls and all twelve lifecycle/RSS controls passed. 1k series2 +7.355493% was below A/A12.715546%,5/6positive; large-case medians remained below A/A. No stable acceleration established.

Reject and restore src/exec.rs to b9f10bc776b05eb5235237e9a900c2ef6aaed1fd. Prototype remains ate3b5e04c7cde545244c2a83cba52bc001ca5be13; all downloaded members/binaries archived and reread against manifest.json. No unchanged numerical retry, main merge, FULL WAL or fresh peer claim. The thread-creation io::Error fallback is a separate robustness concern: current baseline still uses scope.spawn in actual bulk construction while the old fallback helper is unused; preserving that issue does not establish performance benefit or authorize retaining the rejected scheduler. Full eight-peer objective remains unfinished.
