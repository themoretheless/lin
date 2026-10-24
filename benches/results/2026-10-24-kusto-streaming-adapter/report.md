# Kusto native streaming adapter: implemented, live gate pending

Adds insert_native coverage using the streaming REST endpoint. No Rust engine change, real service timings or Kusto win claimed. Current Kusto URL/database/token configuration is absent.

## Protocol and contract

Use the official [streaming HTTP API](https://learn.microsoft.com/en-us/kusto/api/rest/streaming-ingest?view=azure-data-explorer), UTF-8 CSV and encoded database/table path segments. The [inline command](https://learn.microsoft.com/en-us/kusto/management/data-ingestion/ingest-inline?view=microsoft-fabric) is intended for exploration/prototyping and is not selected as the timed high-volume native path. This adapter measures bounded streaming bursts, not saturated queued-ingestion throughput.

Six common fields: id/uri/wing/title/ts:long/body. Input CSV serialization/chunk preparation is outside timing, like existing native prepared inputs. Requests are bounded to 4MiB of encoded bytes (conservative engineering policy, not a claimed optimal or universal service limit); oversized single rows fail before table creation. Record request count, limit and current database streaming policy. Timed span covers all streaming uploads/acknowledgements plus polling until the exact row count is visible, including 10ms poll sleeps and a 60-second visibility deadline. Full six-field/multiplicity readback is outside timing. Noninteger/out-of-range counts, wrong values, HTTP/error/partial-status responses and timeout prevent successful observations. No silent queued fallback or retry-until-win.

Streaming must already be available through the configured cluster/database policy and inherited by new tables. The adapter only reads that policy; it never changes cluster/database/table policies. Live inherited-policy readiness still needs qualification. Multiple requests are not one SQL transaction; SQL uniqueness constraints are not provided. Contract metadata makes the guarantee/index/storage differences explicit. This is an API-specific visible-ingestion comparison with Db::empty, not physical durability equivalence.

## Owned table creation and cleanup

A successful .create table does not prove ownership: [Microsoft documents that it succeeds for an existing table](https://learn.microsoft.com/en-us/kusto/management/create-table-command?view=microsoft-fabric). Both read-fixture and native-fixture setup now use an empty typed datatable through [.set](https://learn.microsoft.com/en-us/kusto/management/data-ingestion/ingest-from-query?view=microsoft-fabric), which rejects existing table names. Only its acknowledged success is tracked as owned. Empty-datatable schema creation must be checked live before timings can count; the tests do not establish its server-side execution. Ownership is not inferred from a racy enumerate-then-create check.

Drop only owned tables after each sample. Failed DROP remains tracked so close retries it, and the original measurement still fails. Existing multi-table cleanup continues attempting every owned table and closes HTTP connection. A server/transport failure may leave a fixture; no universal cleanup guarantee is claimed. No unrelated objects are enumerated/dropped.

## Verification and boundaries

New regression tests failed before implementation. All 17 adapter tests pass. Mocked service cases cover bounded multi-request CSV with Unicode/emoji/quote/comma/tab/newline, partial visibility then success, stream failure, wrong duplicate readback at matching count, existing-name creation failure preservation, invalid/overflow counts, timeout and DROP retry. No policy-change command issued. HTTP helper tests encoded paths, exact payload bytes, success acknowledgement, HTTP 413, error JSON, partial QueryStatus failure and oversized payload rejection before request. These verify client control flow/serialization, not Kusto transport/parser/storage semantics.

A real CLI invocation with Kusto variables deliberately removed returns incomplete/exit 2 with no observations or medians. Original logs and metadata retained. The unchanged Rust workspace/performance matrix was not rerun for this Python-only work. Existing SQLite/DuckDB actual fixtures, MSSQL mock flow and strict-win/incomplete tests remain green.

A dedicated permitted Kusto service remains necessary for small read/streaming/empty-table schema/cleanup qualification, then independent 1k/10k/100k measurements with source/version/settings/raw data and sentinel checks. The emulator is not a benchmark substitute. MSSQL live qualification and durable 1k SQLite gap remain open; full objective incomplete.
