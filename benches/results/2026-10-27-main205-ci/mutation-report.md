# bench 1793044881940276258-3111

Status: Complete

| Case | Metric | Median | Unit | Scope / statistic | Observations | Processes |
|---|---|---:|---|---|---:|---:|
| compare/update_1row_1k/lin [candidate] | wall | 14868.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/update_1row_1k/sqlite [candidate] | wall | 8115.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/delete_1row_1k/lin [candidate] | wall | 18194.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/delete_1row_1k/sqlite [candidate] | wall | 8080.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/update_1row_10k/lin [candidate] | wall | 29655.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/update_1row_10k/sqlite [candidate] | wall | 9773.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/delete_1row_10k/lin [candidate] | wall | 37850.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/delete_1row_10k/sqlite [candidate] | wall | 9487.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/update_1row_100k/lin [candidate] | wall | 49882.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/update_1row_100k/sqlite [candidate] | wall | 17312.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/delete_1row_100k/lin [candidate] | wall | 61835.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/delete_1row_100k/sqlite [candidate] | wall | 18459.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/tx_commit_1row_1k/lin [candidate] | wall | 19927.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/tx_commit_1row_1k/sqlite [candidate] | wall | 10144.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/tx_rollback_1row_1k/lin [candidate] | wall | 27987.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/tx_rollback_1row_1k/sqlite [candidate] | wall | 12182.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/tx_commit_1row_10k/lin [candidate] | wall | 43526.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/tx_commit_1row_10k/sqlite [candidate] | wall | 12458.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/tx_rollback_1row_10k/lin [candidate] | wall | 53640.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/tx_rollback_1row_10k/sqlite [candidate] | wall | 14752.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |

# Useful work throughput

| Case | Median batch throughput | Unit |
|---|---:|---|
| compare/update_1row_1k/lin [candidate] | 67272.0904 | stmts/s |
| compare/update_1row_1k/sqlite [candidate] | 123276.5120 | stmts/s |
| compare/delete_1row_1k/lin [candidate] | 54968.5549 | stmts/s |
| compare/delete_1row_1k/sqlite [candidate] | 123766.2151 | stmts/s |
| compare/update_1row_10k/lin [candidate] | 33721.3147 | stmts/s |
| compare/update_1row_10k/sqlite [candidate] | 102324.0383 | stmts/s |
| compare/delete_1row_10k/lin [candidate] | 26420.3311 | stmts/s |
| compare/delete_1row_10k/sqlite [candidate] | 105401.9736 | stmts/s |
| compare/update_1row_100k/lin [candidate] | 20047.2091 | stmts/s |
| compare/update_1row_100k/sqlite [candidate] | 57791.1353 | stmts/s |
| compare/delete_1row_100k/lin [candidate] | 16172.4799 | stmts/s |
| compare/delete_1row_100k/sqlite [candidate] | 54205.6534 | stmts/s |
| compare/tx_commit_1row_1k/lin [candidate] | 50196.1131 | tx/s |
| compare/tx_commit_1row_1k/sqlite [candidate] | 98576.2057 | tx/s |
| compare/tx_rollback_1row_1k/lin [candidate] | 35731.6417 | tx/s |
| compare/tx_rollback_1row_1k/sqlite [candidate] | 82105.0344 | tx/s |
| compare/tx_commit_1row_10k/lin [candidate] | 22974.9632 | tx/s |
| compare/tx_commit_1row_10k/sqlite [candidate] | 80269.8226 | tx/s |
| compare/tx_rollback_1row_10k/lin [candidate] | 18642.9698 | tx/s |
| compare/tx_rollback_1row_10k/sqlite [candidate] | 67791.5384 | tx/s |

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

