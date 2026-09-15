# Changelog

## [0.3.0] — 2026-09-15

### Stable API freeze

`Queryable` / `BoundQueryable` / `query::pred`, `QueryCursor`, `FromRow` / `LinRow` /
`FromCell` / `map_rows` / `cell_get`, and feature `async` (`AsyncDb` / `AsyncReadDb`,
`to_vec_async` / `stream_*`) move into the **0.3 stable** contract (see crate docs and
README «Стабильный API»).

`async` remains **default-on**. Streams offload work with `spawn_blocking` — this is
not async storage I/O.

Also stable: `Db::with_embedder` / `HashingEmbedder` / `Embedder` (search honesty
unchanged: default hybrid = lex + hashing vec).

### Added

- Fluent `Queryable::search_vec` / `BoundQueryable::search_vec` (parity with DSL
  `search vec`).

### Still experimental

- `ship` (plaintext TCP WAL pull/serve; no TLS/auth/fanout)
- `RecordBatch` / wider OLAP `run_batch`
- `Db::run_stmt`, `RowExt`
- `Stmt` / plan IR internals

### Migration from 0.2.x

1. Bump dependency: `lin = "0.3"` (and `lin-derive` if used directly).
2. No required code changes if you already used Queryable/cursor/async — behavior is
   unchanged; the surface is now covered by the stable list.
3. Prefer fluent `search_vec("…")` instead of only string DSL for vec ranking.
4. Continue to treat `ship` and `RecordBatch` as unstable across minors.

## [0.2.0] — prior

Durable single-writer, backup/FENCE/cold/SyncMode, follower `apply_wal` / bootstrap,
hashing vec + hybrid RRF, Queryable/cursor/async (experimental), TCP `wal-serve` /
`follower sync`, vec WAL v2, soft bench budgets.
