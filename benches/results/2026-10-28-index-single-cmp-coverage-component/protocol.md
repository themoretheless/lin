# Single-comparison index coverage paired component

Candidate b226aa627 versus preceding COW+CAS a7d4b02c3; only src/index.rs differs. Pending SoA and bulk-map candidates are excluded. Same frozen compare.rs, ordinary release binaries, Rust toolchain, data/schema/embedding payload, validation, sync policies and setup/teardown contracts.

Target: compare/filter_eq/lin. Seven controls: Lin filter_range,point_get,text_substr,prepared UPDATE1k,scalar Full UPDATE,and unchanged SQLite/DuckDB filter_eq. Two six-pair series with rotated case/role order, same baseline/control executable, eight observations, max_iterations1,zero warmup. All288fresh processes/2304observations retained. Both target windows require6/6positive pairs and median gain above median absolute A/A. Every control window requires gain>=-max(2%,A/A). No trimming,threshold relaxation or unchanged retry for a pass.

451local release tests/2ignored passed, including432atomic reference comparisons and compound-predicate branch-alignment tests. Linux independently reruns the suite and16fresh correctness smokes before timing. A synthetic complete matrix and missing/negative/losing data checks are analysis tests only, not timing evidence. Component acceptance cannot establish full-engine repair or the complete eight-peer objective; preserve every failing pair and complete source/binary provenance.
