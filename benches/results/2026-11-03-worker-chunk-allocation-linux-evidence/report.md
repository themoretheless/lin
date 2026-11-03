# Worker-local chunk output allocation: rejected

Run https://github.com/themoretheless/lin/actions/runs/41643332237 at e0706061bad53fc9840322b21918d26f5a890857. Independent audit verified 216 processes / 6156 operation samples / 108 lifecycle wall / 108 RSS observations, full runtime/harness/helper closure, actual compilation and both exact ELF files, every exact result/digest and numerical window. Linux workspace 415 passed / zero failed / 3 ignored. All downloaded members archived and reread against SHA-256 manifest. Initial download timed out; only artifact transfer was retried, never numerical measurement.

| Native rows | Series | Median gain % | A/A % | Positive pairs | Passed |
|---:|---:|---:|---:|---:|---|
| 1000 | 1 | 2.277697 | 4.911164 | 4/6 | False |
| 1000 | 2 | -1.532302 | 1.843320 | 3/6 | False |
| 10000 | 1 | -4.422860 | 2.888019 | 0/6 | False |
| 10000 | 2 | -4.440035 | 1.575241 | 0/6 | False |
| 100000 | 1 | -1.052515 | 2.007747 | 2/6 | False |
| 100000 | 2 | -3.304796 | 3.570957 | 0/6 | False |

Zero of six native target windows passed. All twelve prepared read controls and all six RSS controls passed. One mandatory lifecycle wall window failed:

- 1000 rows, series 2: median gain -2.095293%, A/A 1.203087%, allowed loss 2.000000%.

Allocating all output Arcs before feature fill within a worker does not establish a warm or cold benefit. The separate caller-allocation variants had warm wins but failed lifecycle controls; this experiment does not rescue them or justify an unchanged retry. Cross-run differences are not isolated causal evidence about allocator internals.

Restore src/embed.rs exactly to b9f10bc776b05eb5235237e9a900c2ef6aaed1fd; prototype retained at b808ca7b1ee1d5f6103b7ab0ff3497ba75f3a0fe. No main merge, FULL WAL or fresh peer victory. Full eight-peer objective remains open.
