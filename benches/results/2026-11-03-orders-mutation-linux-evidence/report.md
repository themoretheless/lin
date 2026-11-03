# Isolated orders mutations and first SUM: source gate failed

Run https://github.com/themoretheless/lin/actions/runs/41653785707 at a17647493ae1a0162574ddaf09c2755e106ad27f. Candidate runtime b9f10bc776b05eb5235237e9a900c2ef6aaed1fd (f37 equivalent); correctness-aligned baseline a28aa758b774e216f14a6348fb1cf9dca2eb42bc. Native Linux x86_64, Rust 1.96.0, ordinary default-feature release, exact identical example harness. Independent audit verified both source/manifest/harness/ELF closures, actual lib/example compilation, 108 processes / 756 isolated phase observations / 324 complete states and every numerical window. Linux workspace: 414 passed / zero failed / 3 ignored. All artifact members archived and independently reread by SHA-256. Numerical failure, not infrastructure failure.

| Rows | Phase/metric | Series | Median gain % | A/A % | Positive pairs | Passed |
|---:|---|---:|---:|---:|---:|---|
| 1000 | insert | 1 | -2.969216 | 2.684599 | 2/6 | False |
| 1000 | insert | 2 | -1.084752 | 4.044256 | 1/6 | True |
| 1000 | first_sum | 1 | -40.484459 | 21.993510 | 0/6 | False |
| 1000 | first_sum | 2 | -30.383926 | 23.484856 | 0/6 | False |
| 1000 | warm_sum | 1 | 73.294509 | 1.209813 | 6/6 | True |
| 1000 | warm_sum | 2 | 74.253945 | 1.292347 | 6/6 | True |
| 1000 | update | 1 | -3.912314 | 2.086355 | 0/6 | False |
| 1000 | update | 2 | -1.224614 | 4.459694 | 2/6 | True |
| 1000 | after_update_sum | 1 | -64.646264 | 1.572018 | 0/6 | False |
| 1000 | after_update_sum | 2 | -67.526682 | 3.232366 | 0/6 | False |
| 1000 | delete | 1 | -2.675063 | 0.721350 | 1/6 | False |
| 1000 | delete | 2 | -0.600992 | 2.520623 | 3/6 | True |
| 1000 | after_delete_sum | 1 | -62.378657 | 1.901247 | 0/6 | False |
| 1000 | after_delete_sum | 2 | -62.938548 | 5.380166 | 0/6 | False |
| 1000 | process_wall_ns | 1 | -2.829967 | 6.337707 | 2/6 | True |
| 1000 | process_wall_ns | 2 | 0.734679 | 3.578274 | 3/6 | True |
| 1000 | peak_rss_kib | 1 | 0.000000 | 0.000000 | 0/6 | True |
| 1000 | peak_rss_kib | 2 | 0.000000 | 0.000000 | 0/6 | True |
| 10000 | insert | 1 | -1.453395 | 1.387128 | 1/6 | True |
| 10000 | insert | 2 | -1.811806 | 1.023339 | 1/6 | True |
| 10000 | first_sum | 1 | -59.261845 | 2.294085 | 0/6 | False |
| 10000 | first_sum | 2 | -59.617592 | 3.989375 | 0/6 | False |
| 10000 | warm_sum | 1 | 96.657221 | 0.104959 | 6/6 | True |
| 10000 | warm_sum | 2 | 96.627199 | 0.209798 | 6/6 | True |
| 10000 | update | 1 | -0.775554 | 0.403611 | 0/6 | True |
| 10000 | update | 2 | -0.645196 | 0.557686 | 1/6 | True |
| 10000 | after_update_sum | 1 | -76.218161 | 3.142329 | 0/6 | False |
| 10000 | after_update_sum | 2 | -78.183141 | 1.506523 | 0/6 | False |
| 10000 | delete | 1 | -1.055046 | 0.467492 | 1/6 | True |
| 10000 | delete | 2 | -1.012949 | 0.830684 | 1/6 | True |
| 10000 | after_delete_sum | 1 | -76.714313 | 1.427552 | 0/6 | False |
| 10000 | after_delete_sum | 2 | -77.651331 | 1.865778 | 0/6 | False |
| 10000 | process_wall_ns | 1 | -0.584935 | 0.702292 | 2/6 | True |
| 10000 | process_wall_ns | 2 | -0.391062 | 0.753003 | 1/6 | True |
| 10000 | peak_rss_kib | 1 | 0.000000 | 0.000000 | 0/6 | True |
| 10000 | peak_rss_kib | 2 | 0.000000 | 0.000000 | 0/6 | True |
| 100000 | insert | 1 | -2.534521 | 0.844062 | 1/6 | False |
| 100000 | insert | 2 | -0.368801 | 1.341253 | 2/6 | True |
| 100000 | first_sum | 1 | -70.316712 | 3.762338 | 0/6 | False |
| 100000 | first_sum | 2 | -71.299134 | 0.135868 | 0/6 | False |
| 100000 | warm_sum | 1 | 99.590389 | 0.086503 | 6/6 | True |
| 100000 | warm_sum | 2 | 99.574988 | 0.829195 | 6/6 | True |
| 100000 | update | 1 | -1.223077 | 0.640566 | 0/6 | True |
| 100000 | update | 2 | -1.616340 | 0.622702 | 1/6 | True |
| 100000 | after_update_sum | 1 | -76.000927 | 0.488758 | 0/6 | False |
| 100000 | after_update_sum | 2 | -77.707898 | 0.927649 | 0/6 | False |
| 100000 | delete | 1 | 0.387764 | 1.535473 | 4/6 | True |
| 100000 | delete | 2 | -0.101971 | 0.446107 | 3/6 | True |
| 100000 | after_delete_sum | 1 | -76.525863 | 0.694242 | 0/6 | False |
| 100000 | after_delete_sum | 2 | -76.289508 | 0.592497 | 0/6 | False |
| 100000 | process_wall_ns | 1 | -1.185145 | 0.402869 | 0/6 | True |
| 100000 | process_wall_ns | 2 | -0.284274 | 0.277527 | 2/6 | True |
| 100000 | peak_rss_kib | 1 | -0.044016 | 0.026268 | 1/6 | True |
| 100000 | peak_rss_kib | 2 | -0.043306 | 0.045436 | 2/6 | True |

22/54 windows failed. Repeat SUM won every target window (73.29–99.59%, all 6/6 positive). All eighteen first-SUM windows (after INSERT, UPDATE and DELETE) failed: initial 1k/10k/100k regressions 30.38–71.30%; after mutation 62.38–78.18%. Four additional control failures: INSERT 1k series1, UPDATE 1k series1, DELETE 1k series1, INSERT 100k series1. All twelve whole-process/RSS windows passed. The previous lifecycle gate remains a scoped pass; it does not contradict these isolated cold-summary regressions.

Source arithmetic/values remain correct: every scalar and complete row state was independently checked. No rollback to incorrect production main, no relaxing frozen controls, no unchanged numerical retry, and no main merge. The accepted historical warm aggregate gate is warm evidence only; this new broader source qualification fails. Current runtime is retained in the experiment checkout while preparing a changed implementation that avoids repeated cold-summary scans; correctness fixes must be preserved. This result does not establish the exact mechanism until code/profiling confirms it. Eight-peer objective remains incomplete.
