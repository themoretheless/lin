# Combined execution shape/shared keys: rejected

Run https://github.com/themoretheless/lin/actions/runs/41624370562 at71b12b4d64a9fcb51804d0712943c92f2bd0587a completed216 processes/6156operation samples/108lifecycle-wall/108RSS. Independent auditor verified source/harness/helper closures, actual lin/worker compilation for both ordinary ELF binaries, workspace correctness, all exact values/digests and every numerical window. Local workspace417passed/0failed/3ignored. Numerical rejection, not infrastructure failure.

| Native rows | Series | Median gain % | A/A % | Positive pairs | Passed |
|---|---|---:|---:|---|---|
| 1000 | 1 | 0.929446 | 2.237489 | 4/6 | False |
| 1000 | 2 | 0.444837 | 1.255715 | 4/6 | False |
| 10000 | 1 | 3.869429 | 2.966817 | 5/6 | False |
| 10000 | 2 | 5.442885 | 5.244320 | 4/6 | False |
| 100000 | 1 | 12.993365 | 1.460090 | 6/6 | True |
| 100000 | 2 | 12.358943 | 1.340785 | 6/6 | True |

Both100k target windows passed (+12.993365%/+12.358943%,6/6each, aboveA/A1.460090%/1.340785%). Neither1k nor10k passed:1k gains below A/A and4/6positive;10k5/6 and4/6positive. Two read controls failed: join_filter1k series1 -2.622432% versus A/A0.948132%;join_filter10k series2 -2.953314% versus A/A1.146458%. Other ten read controls and all twelve resource windows passed. No additive inference from the separately rejected component results.

Reject combined source and restore src/exec.rs and src/index.rs to b9f10bc776b05eb5235237e9a900c2ef6aaed1fd. Prototype remains atbcf2f27f9db14967daebfad63d6bd0ba63d31b37. Every downloaded binary/artifact member archived and reread against manifest.json. No unchanged numerical retry, main merge, FULL WAL or fresh peer superiority claim. Full eight-peer objective remains unfinished.
