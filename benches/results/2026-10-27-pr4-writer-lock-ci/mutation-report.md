# bench 1793062735827810446-9904

Status: Complete

| Case | Metric | Median | Unit | Scope / statistic | Observations | Processes |
|---|---|---:|---|---|---:|---:|
| compare/update_1row_1k/lin [candidate] | wall | 12368.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/update_1row_1k/sqlite [candidate] | wall | 6009.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/delete_1row_1k/lin [candidate] | wall | 15739.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/delete_1row_1k/sqlite [candidate] | wall | 6049.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/update_1row_10k/lin [candidate] | wall | 25474.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/update_1row_10k/sqlite [candidate] | wall | 10115.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/delete_1row_10k/lin [candidate] | wall | 32238.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/delete_1row_10k/sqlite [candidate] | wall | 12619.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/update_1row_100k/lin [candidate] | wall | 56004.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/update_1row_100k/sqlite [candidate] | wall | 24261.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/delete_1row_100k/lin [candidate] | wall | 65609.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/delete_1row_100k/sqlite [candidate] | wall | 21948.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/tx_commit_1row_1k/lin [candidate] | wall | 15403.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/tx_commit_1row_1k/sqlite [candidate] | wall | 7286.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/tx_rollback_1row_1k/lin [candidate] | wall | 20631.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/tx_rollback_1row_1k/sqlite [candidate] | wall | 9083.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/tx_commit_1row_10k/lin [candidate] | wall | 35603.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/tx_commit_1row_10k/sqlite [candidate] | wall | 12118.5000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/tx_rollback_1row_10k/lin [candidate] | wall | 46255.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |
| compare/tx_rollback_1row_10k/sqlite [candidate] | wall | 19344.0000 | ns | current thread wall clock; scheduler included / batch_total | 8 | 1 |

# Useful work throughput

| Case | Median batch throughput | Unit |
|---|---:|---|
| compare/update_1row_1k/lin [candidate] | 80854.0277 | stmts/s |
| compare/update_1row_1k/sqlite [candidate] | 166418.8847 | stmts/s |
| compare/delete_1row_1k/lin [candidate] | 63536.4446 | stmts/s |
| compare/delete_1row_1k/sqlite [candidate] | 165353.1854 | stmts/s |
| compare/update_1row_10k/lin [candidate] | 39255.9673 | stmts/s |
| compare/update_1row_10k/sqlite [candidate] | 98951.4211 | stmts/s |
| compare/delete_1row_10k/lin [candidate] | 31019.4889 | stmts/s |
| compare/delete_1row_10k/sqlite [candidate] | 79248.7672 | stmts/s |
| compare/update_1row_100k/lin [candidate] | 17882.3946 | stmts/s |
| compare/update_1row_100k/sqlite [candidate] | 41230.2487 | stmts/s |
| compare/delete_1row_100k/lin [candidate] | 15258.2867 | stmts/s |
| compare/delete_1row_100k/sqlite [candidate] | 45566.4459 | stmts/s |
| compare/tx_commit_1row_1k/lin [candidate] | 64923.1018 | tx/s |
| compare/tx_commit_1row_1k/sqlite [candidate] | 137268.2019 | tx/s |
| compare/tx_rollback_1row_1k/lin [candidate] | 48469.6165 | tx/s |
| compare/tx_rollback_1row_1k/sqlite [candidate] | 110089.8435 | tx/s |
| compare/tx_commit_1row_10k/lin [candidate] | 28087.1263 | tx/s |
| compare/tx_commit_1row_10k/sqlite [candidate] | 82536.7742 | tx/s |
| compare/tx_rollback_1row_10k/lin [candidate] | 21619.9940 | tx/s |
| compare/tx_rollback_1row_10k/sqlite [candidate] | 51695.8960 | tx/s |

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

