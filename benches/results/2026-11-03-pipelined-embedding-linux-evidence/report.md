# Pipelined caller allocation: warm win, lifecycle rejection

Run https://github.com/themoretheless/lin/actions/runs/41640680016 at eae3cbe821ec0c0d63eed2cdbe4cbf4aa0b27b03. Independent audit verified all 216 processes / 6156 operation samples / 108 whole-worker wall and 108 RSS observations, full source/harness/helper closure, actual compilation and both exact ELF files, all exact result values/digests and numerical decisions. Linux workspace 415 passed / zero failed / 3 ignored. All downloaded members archived and reread against SHA-256 manifest. Numerical failure, not infrastructure failure.

| Native rows | Series | Median gain % | A/A % | Positive pairs | Passed |
|---:|---:|---:|---:|---:|---|
| 1000 | 1 | 17.945379 | 2.064525 | 6/6 | True |
| 1000 | 2 | 11.222965 | 5.732542 | 6/6 | True |
| 10000 | 1 | 11.349443 | 2.773185 | 6/6 | True |
| 10000 | 2 | 11.666266 | 1.906061 | 6/6 | True |
| 100000 | 1 | 16.481023 | 0.916563 | 6/6 | True |
| 100000 | 2 | 16.936682 | 2.543496 | 6/6 | True |

All six native target and twelve read-control windows passed. Eleven of twelve resource windows passed, including all RSS windows. The mandatory failed resource window:

- 1000 rows, series 1, process_wall_ns: median gain -5.525065%, A/A 1.715720%, allowed loss 2.000000%.

One failed lifecycle control rejects this implementation under the original AND rule. A different host/run from the prior caller-allocation experiment means fewer failed windows cannot establish a causal improvement versus that predecessor. Do not waive the failure, tune a threshold from these observations, or repeat the unchanged candidate seeking acceptance.

Restore src/embed.rs exactly to b9f10bc776b05eb5235237e9a900c2ef6aaed1fd; prototype retained at 330afdb211bd5ead8d5df9ecdff04f2fcf29a169. No main merge, FULL WAL qualification or fresh public/private peer victory is claimed. The eight-peer objective remains open.
