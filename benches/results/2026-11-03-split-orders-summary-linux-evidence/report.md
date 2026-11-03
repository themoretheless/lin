# Independently lazy orders summaries: rejected source gate

Run https://github.com/themoretheless/lin/actions/runs/41659526262 at 2eff9e512ac4fb0af4674dfb27408c37e78f874c. Candidate runtime ec5dfd34ef21309c2a31d8d41fe684c67a6b4d86 versus correctness-aligned a28aa758b774e216f14a6348fb1cf9dca2eb42bc. Exact same example harness as prior failed gate. Native Linux x86_64/Rust 1.96.0/default release. Independent auditor verified all runtime/manifests/harness/ELF closures, actual compilation, 108 processes / 756 isolated phases / 324 complete states and all 54 windows. Linux workspace 415 passed / zero failed / 3 ignored. Every artifact member archived and reread against SHA-256 manifest. Numerical failure, not infrastructure failure.

| Rows | Phase/metric | Series | Median gain % | A/A % | Positive pairs | Passed |
|---:|---|---:|---:|---:|---:|---|
| 1000 | insert | 1 | -1.958036 | 4.536473 | 1/6 | True |
| 1000 | insert | 2 | -4.723762 | 1.189024 | 0/6 | False |
| 1000 | first_sum | 1 | 0.882699 | 6.953211 | 3/6 | True |
| 1000 | first_sum | 2 | -18.248248 | 13.544655 | 1/6 | False |
| 1000 | warm_sum | 1 | 73.519352 | 1.523448 | 6/6 | True |
| 1000 | warm_sum | 2 | 72.504892 | 0.538693 | 6/6 | True |
| 1000 | update | 1 | -2.386108 | 1.463929 | 1/6 | False |
| 1000 | update | 2 | -1.011076 | 2.929789 | 2/6 | True |
| 1000 | after_update_sum | 1 | -1.482814 | 3.200157 | 3/6 | True |
| 1000 | after_update_sum | 2 | -10.381507 | 3.749570 | 0/6 | False |
| 1000 | delete | 1 | 0.566147 | 3.173790 | 5/6 | True |
| 1000 | delete | 2 | 0.237377 | 3.465434 | 4/6 | True |
| 1000 | after_delete_sum | 1 | -2.838285 | 3.779425 | 0/6 | True |
| 1000 | after_delete_sum | 2 | -3.885457 | 2.508144 | 2/6 | False |
| 1000 | process_wall_ns | 1 | 0.149915 | 1.530536 | 3/6 | True |
| 1000 | process_wall_ns | 2 | -2.666018 | 2.469510 | 1/6 | False |
| 1000 | peak_rss_kib | 1 | 0.000000 | 0.000000 | 0/6 | True |
| 1000 | peak_rss_kib | 2 | 0.000000 | 0.000000 | 0/6 | True |
| 10000 | insert | 1 | -0.376685 | 1.645147 | 2/6 | True |
| 10000 | insert | 2 | -1.117995 | 1.090625 | 2/6 | True |
| 10000 | first_sum | 1 | -4.604734 | 4.733277 | 1/6 | True |
| 10000 | first_sum | 2 | -13.353996 | 13.159319 | 1/6 | False |
| 10000 | warm_sum | 1 | 96.813790 | 0.240471 | 6/6 | True |
| 10000 | warm_sum | 2 | 96.931379 | 0.180051 | 6/6 | True |
| 10000 | update | 1 | 3.330708 | 7.580971 | 5/6 | True |
| 10000 | update | 2 | -11.364538 | 13.014765 | 2/6 | True |
| 10000 | after_update_sum | 1 | 1.123688 | 2.704198 | 3/6 | True |
| 10000 | after_update_sum | 2 | -11.331532 | 8.157414 | 2/6 | False |
| 10000 | delete | 1 | 3.609301 | 3.835817 | 4/6 | True |
| 10000 | delete | 2 | -0.323846 | 7.769942 | 3/6 | True |
| 10000 | after_delete_sum | 1 | -3.420846 | 2.938250 | 2/6 | False |
| 10000 | after_delete_sum | 2 | -5.261851 | 5.842364 | 2/6 | True |
| 10000 | process_wall_ns | 1 | 1.144561 | 1.046674 | 4/6 | True |
| 10000 | process_wall_ns | 2 | -3.448372 | 3.273152 | 1/6 | False |
| 10000 | peak_rss_kib | 1 | 0.000000 | 0.000000 | 0/6 | True |
| 10000 | peak_rss_kib | 2 | 0.000000 | 0.000000 | 0/6 | True |
| 100000 | insert | 1 | -2.529522 | 3.503260 | 2/6 | True |
| 100000 | insert | 2 | -5.286489 | 2.291243 | 0/6 | False |
| 100000 | first_sum | 1 | -1.682981 | 0.521451 | 0/6 | True |
| 100000 | first_sum | 2 | -0.741214 | 1.332676 | 2/6 | True |
| 100000 | warm_sum | 1 | 99.638488 | 0.123819 | 6/6 | True |
| 100000 | warm_sum | 2 | 99.640999 | 0.038034 | 6/6 | True |
| 100000 | update | 1 | -0.715293 | 2.791521 | 2/6 | True |
| 100000 | update | 2 | -0.156262 | 1.141510 | 3/6 | True |
| 100000 | after_update_sum | 1 | -1.038200 | 0.778389 | 2/6 | True |
| 100000 | after_update_sum | 2 | -0.075894 | 1.407374 | 3/6 | True |
| 100000 | delete | 1 | 0.742058 | 0.743271 | 3/6 | True |
| 100000 | delete | 2 | 1.401219 | 2.538263 | 4/6 | True |
| 100000 | after_delete_sum | 1 | -0.433298 | 0.602253 | 1/6 | True |
| 100000 | after_delete_sum | 2 | 0.290062 | 1.095741 | 4/6 | True |
| 100000 | process_wall_ns | 1 | -0.566029 | 1.463592 | 2/6 | True |
| 100000 | process_wall_ns | 2 | -1.320455 | 0.946489 | 1/6 | True |
| 100000 | peak_rss_kib | 1 | -0.020588 | 0.046145 | 3/6 | True |
| 100000 | peak_rss_kib | 2 | -0.066053 | 0.045435 | 1/6 | True |

11/54 windows failed. All six warm-SUM targets pass (72.50–99.64%, 6/6 positive each). All six cold-summary windows at 100k pass; their medians are -1.683 to +0.290% versus correct baseline. Thus this implementation avoids the large cold-summary regression in the 100k tested fixture, but smaller first/post-mutation SUM windows still fail. Additional failures: INSERT 1k/100k series2, UPDATE 1k series1, whole-process wall 1k/10k series2. All six RSS controls pass. Full gate rejects this implementation; partial wins cannot waive failed controls.

Cross-run comparison with the prior f37 gate is contextual, not a same-host paired causal estimate of the split change. The implementation removes an objectively unused scan, but this source has not met end-to-end acceptance. Restore exec.rs/store.rs exactly to b9f10bc776b05eb5235237e9a900c2ef6aaed1fd while retaining correctness; prototype remains at ec5dfd34ef21309c2a31d8d41fe684c67a6b4d86. Do not revert to incorrect production main, merge this unqualified source, relax controls or retry unchanged. No fresh peer or FULL WAL qualification. Full eight-peer objective remains incomplete.
