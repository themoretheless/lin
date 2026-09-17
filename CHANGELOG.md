# Changelog

## [Unreleased]

### Added

- **Durable FTS:** checkpoint writes `fts/<collection>.bin` (`LIN\x05` posting lists).
  Open loads matching gen/nrows/fields and incrementally applies the WAL tail;
  missing or stale blobs still rebuild.
- **Ship:** protocol `LIN\x06` + shared token; optional rustls (`--tls-cert` /
  `--tls-key` / `--tls-ca`); one live sink. Loopback plaintext remains the local recipe.
- Dapper/ADO verbs: `Db::execute` (`n`+`gen`), `Db::scalar` / `Queryable::scalar`,
  `first` / `first_or` / `single` (explicit `take 1`/`2`, not implicit 50),
  `BoundQueryable::cursor`, `buffered` alias for `to_vec`.
  Typed: `first_typed` / `first_or_typed` / `single_typed`, `QueryCursor::next_typed`,
  `Handle::{first,first_typed,rows_typed}`. `ToCell`/`FromCell`
  for `Option`, `Arc<str>`, `Vec<f32>` / `Arc<[f32]>`; `cell_opt`.
- `#[lin(rename = "users.email")]` now parses (`owned` no longer dropped the `rename` value).
- Compact `ProjectedRow` cursor output via `QueryCursor::next_projected`.
- Catalog `filter` / `unfilter` (read-path global pred) and `docs all` / `Queryable::ignore_filters`.
- `Queryable::prepare` / `Db::prepare_query` cache Query AST → `Prepared` (no typecheck on hit).
- `owned` value objects flatten onto a collection; `ToCell` + `#[lin(owned)]` nested `FromRow`.
- `Db::run_group` atomic group commit and real-open phase metrics in `Stats::reopen`.
- Batch embedder hook; hashing embed reuses lowercase scratch across slabs.

### Performance

- FTS lex `take` uses borrowed `(score, row_idx)` top-k instead of cloning and
  fully sorting all candidate rows.
- Reopen no longer builds row maps and scalar indexes twice around WAL replay.
- Profile-backed `#[inline]` and bounds-check removal in the aligned SoA cursor loop.

## [0.4.0] — 2026-09-16

### Added

- **m2 FTS:** in-memory postings (`src/fts.rs`) for catalog fields with `fts: true`
  (`docs.title` / `docs.body`). Rebuild-on-open / after writes (no durable FTS blob).
  `search lex` and hybrid lex-half use postings → residual `lex_score`. Plan/explain
  label **`FtsSeek`** (not `IndexSeek`).
- **m5 lazy cursor:** `hop` depth=1 and `search lex|hybrid | take` are lazy
  (`QueryCursor::is_lazy()`), driven by FTS/neighbor idxs.

### Docs

- README roadmap: 0.4 done; guarantees table lists FTS; lazy cursor shapes updated.

## [0.3.1] — 2026-09-16

### Added

- **m1 (opt-in):** feature `embed-ollama` → [`OllamaEmbedder`] (HTTP `/api/embeddings`);
  feature `embed-onnx` → [`OnnxEmbedder`] (fastembed / ONNX Runtime). Same [`Embedder`]
  trait; wire with `Db::with_embedder`. **Default remains [`HashingEmbedder`]**.
- **m4:** `run_batch` columnar path for `filter? | project | skip* | take?`
  (`try_filter_project_batch`) without row-map materialization; join path unchanged.

### Performance

- Join SoA (`orders ⋈ users`): probe by user index (no per-user `Vec<Cell>`); unrolled
  hot project `{ id, users.email, total }` (~24% faster `join_inner/lin` on quick profile).
- Lazy join cursor: SoA path for the same hot shape (`LazyJoinSoa`) — ~3.0 ms → ~1.7 ms
  on `join_inner/lin_cursor`.
- Materialize: compact `{ id, title }` row builder (`row_id_title`).

### Honesty

Neural backends are feature-gated and never the default. `search` / hybrid without
`with_embedder` still use local hashing vec. ONNX first use may download model/
runtime weights.

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
