# bench 1793041276748207233-9899

Status: Complete

| Case | Metric | Median | Unit | Scope / statistic | Observations | Processes |
|---|---|---:|---|---|---:|---:|
| compare/update_1row_1k/lin [candidate] | wall | 8703.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/update_1row_1k/sqlite [candidate] | wall | 3470.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/delete_1row_1k/lin [candidate] | wall | 10416.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/delete_1row_1k/sqlite [candidate] | wall | 4202.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/update_1row_10k/lin [candidate] | wall | 24487.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/update_1row_10k/sqlite [candidate] | wall | 10230.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/delete_1row_10k/lin [candidate] | wall | 30701.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/delete_1row_10k/sqlite [candidate] | wall | 8247.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/update_1row_100k/lin [candidate] | wall | 48628.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/update_1row_100k/sqlite [candidate] | wall | 27106.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/delete_1row_100k/lin [candidate] | wall | 55734.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/delete_1row_100k/sqlite [candidate] | wall | 25012.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/tx_commit_1row_1k/lin [candidate] | wall | 11672.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/tx_commit_1row_1k/sqlite [candidate] | wall | 4347.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/tx_rollback_1row_1k/lin [candidate] | wall | 13795.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/tx_rollback_1row_1k/sqlite [candidate] | wall | 4812.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/tx_commit_1row_10k/lin [candidate] | wall | 30826.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/tx_commit_1row_10k/sqlite [candidate] | wall | 6474.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/tx_rollback_1row_10k/lin [candidate] | wall | 39484.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/tx_rollback_1row_10k/sqlite [candidate] | wall | 10345.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |

# Useful work throughput

| Case | Median batch throughput | Unit |
|---|---:|---|
| compare/update_1row_1k/lin [candidate] | 114905.3343 | stmts/s |
| compare/update_1row_1k/sqlite [candidate] | 288232.9121 | stmts/s |
| compare/delete_1row_1k/lin [candidate] | 96023.4917 | stmts/s |
| compare/delete_1row_1k/sqlite [candidate] | 238038.8724 | stmts/s |
| compare/update_1row_10k/lin [candidate] | 40844.5418 | stmts/s |
| compare/update_1row_10k/sqlite [candidate] | 97752.1170 | stmts/s |
| compare/delete_1row_10k/lin [candidate] | 32580.3933 | stmts/s |
| compare/delete_1row_10k/sqlite [candidate] | 123045.1450 | stmts/s |
| compare/update_1row_100k/lin [candidate] | 20566.8768 | stmts/s |
| compare/update_1row_100k/sqlite [candidate] | 36894.5222 | stmts/s |
| compare/delete_1row_100k/lin [candidate] | 17942.3899 | stmts/s |
| compare/delete_1row_100k/sqlite [candidate] | 39981.5553 | stmts/s |
| compare/tx_commit_1row_1k/lin [candidate] | 85856.1750 | tx/s |
| compare/tx_commit_1row_1k/sqlite [candidate] | 230396.0740 | tx/s |
| compare/tx_rollback_1row_1k/lin [candidate] | 72487.4130 | tx/s |
| compare/tx_rollback_1row_1k/sqlite [candidate] | 207792.4792 | tx/s |
| compare/tx_commit_1row_10k/lin [candidate] | 32439.6535 | tx/s |
| compare/tx_commit_1row_10k/sqlite [candidate] | 154454.2929 | tx/s |
| compare/tx_rollback_1row_10k/lin [candidate] | 25336.0030 | tx/s |
| compare/tx_rollback_1row_10k/sqlite [candidate] | 96672.3725 | tx/s |

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

