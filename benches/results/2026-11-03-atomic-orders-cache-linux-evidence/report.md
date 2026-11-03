# Atomic orders summary cache: rejected source gate

Run https://github.com/themoretheless/lin/actions/runs/41664896040 at 80f86f5aeaff3eadc24683db075784c8bbfacb10. Candidate runtime 7bd59e4d2f6071a8c98bc7a82a2ca7273a5ad91c versus correctness-aligned a28aa758b774e216f14a6348fb1cf9dca2eb42bc. Native Linux x86_64 / Rust 1.96.0 / ordinary default release. Independent auditor verified runtime/manifests/harness/ELF closure, actual compilation, 108 processes, 756 isolated phases, 324 complete row states, and all 54 windows. Workspace: 415 passed, 0 failed, 3 ignored. Full artifact archived and every member reread against SHA-256.

**Rejected: 7 of 54 windows fail. All six warm SUM targets pass, but fixed controls are binding. Numerical failure, not infrastructure failure.**

| Rows | Metric | Series | Median gain % | A/A % | Positive pairs | Pass |
|---:|---|---:|---:|---:|---:|---|
| 1000 | insert | 1 | -0.875449 | 1.262034 | 2/6 | True |
| 1000 | insert | 2 | -2.851729 | 1.305616 | 1/6 | False |
| 1000 | first_sum | 1 | -11.057766 | 7.419342 | 2/6 | False |
| 1000 | first_sum | 2 | -12.233967 | 10.054694 | 1/6 | False |
| 1000 | warm_sum | 1 | 67.076989 | 0.835436 | 6/6 | True |
| 1000 | warm_sum | 2 | 70.714288 | 0.835436 | 6/6 | True |
| 1000 | update | 1 | 1.147958 | 0.711035 | 4/6 | True |
| 1000 | update | 2 | -1.407472 | 2.874806 | 2/6 | True |
| 1000 | after_update_sum | 1 | -1.134644 | 3.043729 | 2/6 | True |
| 1000 | after_update_sum | 2 | -2.714223 | 3.214929 | 1/6 | True |
| 1000 | delete | 1 | -4.900174 | 2.292517 | 0/6 | False |
| 1000 | delete | 2 | 0.256705 | 2.641268 | 4/6 | True |
| 1000 | after_delete_sum | 1 | -4.636187 | 6.357408 | 1/6 | True |
| 1000 | after_delete_sum | 2 | -2.784486 | 5.474937 | 3/6 | True |
| 1000 | process_wall_ns | 1 | -1.465126 | 3.707608 | 2/6 | True |
| 1000 | process_wall_ns | 2 | -2.929101 | 1.946563 | 2/6 | False |
| 1000 | peak_rss_kib | 1 | 0.000000 | 0.000000 | 0/6 | True |
| 1000 | peak_rss_kib | 2 | 0.000000 | 0.000000 | 0/6 | True |
| 10000 | insert | 1 | 0.054802 | 1.456271 | 3/6 | True |
| 10000 | insert | 2 | -1.405471 | 2.081080 | 2/6 | True |
| 10000 | first_sum | 1 | 2.569205 | 3.920965 | 3/6 | True |
| 10000 | first_sum | 2 | -1.787415 | 4.281294 | 2/6 | True |
| 10000 | warm_sum | 1 | 96.424429 | 0.468248 | 6/6 | True |
| 10000 | warm_sum | 2 | 96.489687 | 0.267218 | 6/6 | True |
| 10000 | update | 1 | -1.082781 | 5.001217 | 2/6 | True |
| 10000 | update | 2 | 0.300264 | 2.618125 | 3/6 | True |
| 10000 | after_update_sum | 1 | -1.598502 | 5.076564 | 3/6 | True |
| 10000 | after_update_sum | 2 | -1.379740 | 4.865202 | 2/6 | True |
| 10000 | delete | 1 | -0.954608 | 1.948303 | 2/6 | True |
| 10000 | delete | 2 | -3.018107 | 2.602700 | 1/6 | False |
| 10000 | after_delete_sum | 1 | 3.666185 | 3.219820 | 5/6 | True |
| 10000 | after_delete_sum | 2 | -1.643293 | 3.972573 | 2/6 | True |
| 10000 | process_wall_ns | 1 | -0.761828 | 1.613015 | 2/6 | True |
| 10000 | process_wall_ns | 2 | -1.076403 | 2.428410 | 2/6 | True |
| 10000 | peak_rss_kib | 1 | 0.000000 | 0.000000 | 0/6 | True |
| 10000 | peak_rss_kib | 2 | 0.000000 | 0.000000 | 0/6 | True |
| 100000 | insert | 1 | -1.672213 | 1.540857 | 2/6 | True |
| 100000 | insert | 2 | -0.484755 | 2.872692 | 3/6 | True |
| 100000 | first_sum | 1 | -0.583830 | 0.990164 | 2/6 | True |
| 100000 | first_sum | 2 | -0.570227 | 0.367982 | 2/6 | True |
| 100000 | warm_sum | 1 | 99.510016 | 0.027410 | 6/6 | True |
| 100000 | warm_sum | 2 | 99.553672 | 0.177182 | 6/6 | True |
| 100000 | update | 1 | -0.916888 | 1.827237 | 1/6 | True |
| 100000 | update | 2 | -2.663372 | 1.175233 | 0/6 | False |
| 100000 | after_update_sum | 1 | -0.088605 | 6.779108 | 2/6 | True |
| 100000 | after_update_sum | 2 | -0.747026 | 1.240642 | 2/6 | True |
| 100000 | delete | 1 | 1.093031 | 1.213949 | 6/6 | True |
| 100000 | delete | 2 | 0.925656 | 0.923538 | 4/6 | True |
| 100000 | after_delete_sum | 1 | 0.358901 | 0.487873 | 4/6 | True |
| 100000 | after_delete_sum | 2 | -0.553108 | 0.607145 | 2/6 | True |
| 100000 | process_wall_ns | 1 | -1.065019 | 0.857571 | 1/6 | True |
| 100000 | process_wall_ns | 2 | -1.488194 | 0.866772 | 1/6 | True |
| 100000 | peak_rss_kib | 1 | -0.067433 | 0.056808 | 1/6 | True |
| 100000 | peak_rss_kib | 2 | -0.090913 | 0.001421 | 0/6 | True |

Positive gain means faster (or lower RSS). Target requires all six positive pairs and median gain above independent A/A noise. Other time windows tolerate -max(2%, A/A); RSS tolerates -max(5%, A/A). All windows are ANDed.

Prototype retained in runtime commit above. Restore only src/exec.rs and src/store.rs to b9f10bc776b05eb5235237e9a900c2ef6aaed1fd after archive verification; no main merge, unchanged numerical retry, or threshold change. This rejection does not establish any public-peer win and does not complete the eight-peer objective.
