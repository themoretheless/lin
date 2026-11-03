# Worker-local embedding text buffers: rejected

Run https://github.com/themoretheless/lin/actions/runs/41620139103 at858e394b8c076325e5835565531c4b7b900d4a0a completed216 processes/6156 operation samples/108 lifecycle-wall/108 RSS. Independent audit verified source/harness/helper closures, actual lin/worker compilation for both ELF binaries, workspace tests, all exact result readbacks/digests and numeric windows. Local workspace417 passed/0failed/3ignored. This is numerical rejection, not infrastructure failure.

| Native rows | Series | Median gain % | A/A % | Positive pairs | Passed |
|---|---|---:|---:|---|---|
| 1000 | 1 | 2.468969 | 2.909054 | 5/6 | False |
| 1000 | 2 | 4.915291 | 2.402600 | 5/6 | False |
| 10000 | 1 | -1.313655 | 6.467627 | 2/6 | False |
| 10000 | 2 | 4.483895 | 3.750383 | 4/6 | False |
| 100000 | 1 | 5.679753 | 2.588569 | 6/6 | True |
| 100000 | 2 | 2.892626 | 3.327997 | 5/6 | False |

Only one of six native target windows passed:100k series1 +5.679753% versus A/A2.588569%,6/6positive. Series2 +2.892626% was below A/A3.327997% with5/6positive. Two of twelve read controls failed: join_filter1k series1 -3.695638% (A/A0.382297%) and series2 -3.728537% (A/A0.700888%). All twelve lifecycle/RSS windows passed. Do not infer causal unrelated read regression, waive frozen controls, or promote nonrepeated positive medians into qualification.

Reject and restore both src/embed.rs and src/exec.rs to b9f10bc776b05eb5235237e9a900c2ef6aaed1fd. Source prototype and additive trait method remain recoverable at7d239c114ea453f7501e627f8c33fd6b9eec9363; neither is retained in current runtime. Every downloaded binary/raw artifact is archived and reread against manifest.json. No unchanged numerical retry, main merge or fresh peer superiority claim. No FULL WAL timing for this source. The eight-peer objective remains unfinished.
