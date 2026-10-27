# Incremental reverse/unique order warm diagnostic

Six rotated A/A/B rounds,18processes;16samples of512alternating prepared updates and one explicit rollback per size. Baseline and candidate both integrate COW, take-all and Eq/Hash on main047; only index.rs check ordering differs (see variant hashes). Exact readback after every sample.

UPDATE1k/10k/100k -0.679/-0.887/-1.088%, A/A0.570/0.954/1.007%, positive pairs0/6,2/6,2/6. Rollback0.000/-0.376/+1.068%. No accepted improvement. This warm diagnostic is not fresh-fixture or peer qualification. Retain all outliers, no unchanged retry. Production source restored.
