# bench 1793063479980573446-10214

Status: Complete

| Case | Metric | Median | Unit | Scope / statistic | Observations | Processes |
|---|---|---:|---|---|---:|---:|
| compare/update_1row_1k/lin [candidate] | wall | 19471.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/update_1row_1k/sqlite [candidate] | wall | 9678.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/delete_1row_1k/lin [candidate] | wall | 20498.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/delete_1row_1k/sqlite [candidate] | wall | 8947.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/update_1row_10k/lin [candidate] | wall | 30251.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/update_1row_10k/sqlite [candidate] | wall | 14992.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/delete_1row_10k/lin [candidate] | wall | 40295.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/delete_1row_10k/sqlite [candidate] | wall | 15680.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/update_1row_100k/lin [candidate] | wall | 62251.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/update_1row_100k/sqlite [candidate] | wall | 35616.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/delete_1row_100k/lin [candidate] | wall | 72005.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/delete_1row_100k/sqlite [candidate] | wall | 32110.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/tx_commit_1row_1k/lin [candidate] | wall | 19662.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/tx_commit_1row_1k/sqlite [candidate] | wall | 10464.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/tx_rollback_1row_1k/lin [candidate] | wall | 30372.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/tx_rollback_1row_1k/sqlite [candidate] | wall | 12523.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/tx_commit_1row_10k/lin [candidate] | wall | 44864.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/tx_commit_1row_10k/sqlite [candidate] | wall | 20568.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/tx_rollback_1row_10k/lin [candidate] | wall | 58534.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/tx_rollback_1row_10k/sqlite [candidate] | wall | 22932.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |

# Useful work throughput

| Case | Median batch throughput | Unit |
|---|---:|---|
| compare/update_1row_1k/lin [candidate] | 51360.3873 | stmts/s |
| compare/update_1row_1k/sqlite [candidate] | 103402.5657 | stmts/s |
| compare/delete_1row_1k/lin [candidate] | 48784.6344 | stmts/s |
| compare/delete_1row_1k/sqlite [candidate] | 111776.1503 | stmts/s |
| compare/update_1row_10k/lin [candidate] | 33056.8027 | stmts/s |
| compare/update_1row_10k/sqlite [candidate] | 66720.9406 | stmts/s |
| compare/delete_1row_10k/lin [candidate] | 24817.4100 | stmts/s |
| compare/delete_1row_10k/sqlite [candidate] | 63777.6114 | stmts/s |
| compare/update_1row_100k/lin [candidate] | 16066.5666 | stmts/s |
| compare/update_1row_100k/sqlite [candidate] | 28078.0505 | stmts/s |
| compare/delete_1row_100k/lin [candidate] | 13889.7346 | stmts/s |
| compare/delete_1row_100k/sqlite [candidate] | 31143.5382 | stmts/s |
| compare/tx_commit_1row_1k/lin [candidate] | 50861.6147 | tx/s |
| compare/tx_commit_1row_1k/sqlite [candidate] | 95574.9296 | tx/s |
| compare/tx_rollback_1row_1k/lin [candidate] | 32925.9201 | tx/s |
| compare/tx_rollback_1row_1k/sqlite [candidate] | 79869.5713 | tx/s |
| compare/tx_commit_1row_10k/lin [candidate] | 22289.6040 | tx/s |
| compare/tx_commit_1row_10k/sqlite [candidate] | 48619.2258 | tx/s |
| compare/tx_rollback_1row_10k/lin [candidate] | 17086.0912 | tx/s |
| compare/tx_rollback_1row_10k/sqlite [candidate] | 43606.3128 | tx/s |

Derived from positive wall batches and declared units per operation; zero/unavailable durations omitted.

Medians above summarize observations, not pooled operation-latency percentiles. Comparisons aggregate within each process first.

- Raw batches are not independent process replications. Batch averages are not individual-operation latency. CPU frequency and background load are uncontrolled.

- compare/update_1row_1k/lin: some samples below half the requested duration; iteration cap or workload drift may limit precision

- compare/update_1row_1k/sqlite: some samples below half the requested duration; iteration cap or workload drift may limit precision

- compare/delete_1row_1k/lin: some samples below half the requested duration; iteration cap or workload drift may limit precision

- compare/delete_1row_1k/sqlite: some samples below half the requested duration; iteration cap or workload drift may limit precision

- compare/update_1row_10k/lin: some samples below half the requested duration; iteration cap or workload drift may limit precision

- compare/update_1row_10k/sqlite: some samples below half the requested duration; iteration cap or workload drift may limit precision

- compare/delete_1row_10k/lin: some samples below half the requested duration; iteration cap or workload drift may limit precision

- compare/delete_1row_10k/sqlite: some samples below half the requested duration; iteration cap or workload drift may limit precision

- compare/update_1row_100k/lin: some samples below half the requested duration; iteration cap or workload drift may limit precision

- compare/update_1row_100k/sqlite: some samples below half the requested duration; iteration cap or workload drift may limit precision

- compare/delete_1row_100k/lin: some samples below half the requested duration; iteration cap or workload drift may limit precision

- compare/delete_1row_100k/sqlite: some samples below half the requested duration; iteration cap or workload drift may limit precision

- compare/tx_commit_1row_1k/lin: some samples below half the requested duration; iteration cap or workload drift may limit precision

- compare/tx_commit_1row_1k/sqlite: some samples below half the requested duration; iteration cap or workload drift may limit precision

- compare/tx_rollback_1row_1k/lin: some samples below half the requested duration; iteration cap or workload drift may limit precision

- compare/tx_rollback_1row_1k/sqlite: some samples below half the requested duration; iteration cap or workload drift may limit precision

- compare/tx_commit_1row_10k/lin: some samples below half the requested duration; iteration cap or workload drift may limit precision

- compare/tx_commit_1row_10k/sqlite: some samples below half the requested duration; iteration cap or workload drift may limit precision

- compare/tx_rollback_1row_10k/lin: some samples below half the requested duration; iteration cap or workload drift may limit precision

- compare/tx_rollback_1row_10k/sqlite: some samples below half the requested duration; iteration cap or workload drift may limit precision

