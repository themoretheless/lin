# PostgreSQL native benchmark fixture isolation

Native helpers previously dropped fixed names in the default schema. Each PostgreSQL fixture now creates one uniquely named persistent schema, sets search_path to it, and removes the owned schema when the fixture client drops. Warm reads, joins, bulk docs and logs share this ownership wrapper. Ordinary WAL-logged tables, transaction semantics and benchmark timer boundaries retained. Requires CREATE privilege on the test database. Cleanup failures are reported.

Added LIN_BENCH_SKIP_MYSQL to suppress MySQL connection/setup during independent PostgreSQL runs. MySQL isolation remains outstanding; do not infer this change makes that helper safe. The separate Python peer harness already uses owned table names.

## Live verification

Compiled the native benchmark offline. Started only the pre-existing owned lin-bench-postgres-1 container, created a unique validation database, and created five permanent sentinel tables in public: docs, docs_bulk, users, orders, logs_bulk. Ran all 11 PostgreSQL cases with one sample/operation each. Every case completed. All five sentinel values remained identical; zero lin_bench_* schemas remained. Exact evidence saved in verification.json and validation/run.json. Validation database removed and owned container stopped afterward.

This run verifies isolation and execution coverage, not performance superiority, timing stability, or full peer write equivalence. Real performance runs still required; native PostgreSQL bulk/log readback validation remains to strengthen. No complete result for MSSQL/Kusto yet. The full all-eight-peer goal remains active and unproven. Source and binary hashes saved.
