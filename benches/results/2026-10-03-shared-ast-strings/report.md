# Shared AST string literals

Retained change: experimental ast::Value::String stores Arc<str> instead of
String. Parser/fluent constructors own shared literals; value_cell and index
literal conversion clone their Arc rather than allocate another text payload.
Stable Row remains BTreeMap<String, Cell>, Cell remains unchanged. Relative
execution-time values, duplicate-field last-write semantics and source/plan
rendering remain covered by workspace tests. This is distinct from the previously
rejected prepared side cache: no extra per-record cache or lookup is retained.

Compatibility: manual experimental AST constructors that pass an owned String
must use Value::String(text.into()). Constructors already using .into() and
stable text/query APIs continue to work. Experimental AST/Stmt/plan IR are outside
the stable API freeze documented in src/lib.rs. No CLI or WAL format change.

Six alternating independent process pairs per write mode, 24 fresh fixtures per
case, one timed operation, no warmup. Default embedding, FTS, scalar indexes,
identity/URI constraints and Full sync retained. Exact readback/affected-count
gates passed. Preparation/schema/validation/drop excluded according to existing
case contracts. Native Handle output drop remains inside timing; durable Handle
drop remains outside timing. No builds/tests overlap performance runs. Host load
and filesystem scheduling uncontrolled. Aggregate process medians, not pooled
operation latency; paired gain is median of per-pair relative changes.

| Lin case | Baseline ms | Candidate ms | Paired gain | Wins |
|---|---:|---:|---:|---:|
| Native 1k | 1.318157 | 1.244375 | 5.25% | 6/6 |
| Native 10k | 14.004823 | 13.215157 | 5.63% | 6/6 |
| Durable 1k | 3.260698 | 3.123625 | 1.44% | 3/6 |
| Durable 10k | 25.790479 | 24.796844 | 3.30% | 6/6 |

Small durable improvement is inconclusive. SQLite controls: native 1k -0.53%,
native 10k -0.01%, durable 1k -1.67%, durable 10k -0.75% paired gain. Candidate
Lin beats SQLite native 10k in 6/6 processes; loses native 1k and both durable
sizes in 6/6. This does not prove the full eight-engine objective.

Read checks: three alternating independent pairs, 32 warm samples, 50ms warmup,
5ms target, max 100 operations/sample. FTS concern expanded to six pairs.

| Lin read | Paired gain | Wins |
|---|---:|---:|
| Point get | 0.00% | 1/3 (one tie) |
| Equality filter | 6.89% | 3/3 |
| Range filter | 9.64% | 3/3 |
| Text substring | 4.82% | 3/3 |
| Materialize | 0.70% | 3/3 |
| FTS common, expanded six pairs | -2.19% | 2/6 |

Known tradeoff: FTS common is slower in four of six pairs. The mechanism is not
established by wall-clock measurements. The change is retained for consistent
native/large durable insertion and filter gains; FTS remains an open performance
issue, and no universal speedup is claimed. Submicrosecond/very small changes
should be treated cautiously. Read results are before/after Lin checks, not
refreshed peer-engine wins. MSSQL/Kusto endpoints remain unavailable.

Validation: cargo check --offline --all-targets, full cargo test --offline
--workspace, cargo check --offline --features gpu --all-targets passed. New
regression verifies Unicode literal storage is shared with the prepared AST and
survives source/plan destruction. GPU runtime performance is not measured.
Baseline/candidate benchmark builds pass; source/binary SHA256, source copies,
raw observations and process medians retained. Trailing log blank lines normalized;
raw JSON unchanged. Scoped rustfmt and source diff checks pass.
