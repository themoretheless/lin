# Query Semantics Reference and Runnable Examples

This reference documents Lin's exact evaluation rules—including sparse `Null` vs absent fields, `i64`/`f64`/`NaN` numeric boundaries, exact vs approximate search, and CPU/GPU parity—alongside runnable examples (`LIN-1964`–`LIN-1968`, `LIN-1985`–`LIN-1988`).

---

## 1. Core Semantics Reference (`LIN-1964`)

### 1.1 `Cell::Null` vs Absent Sparse Fields (`LIN-1967`)
- **Storage model:** `Row` is sparse—an omitted field on `insert` is not stored in the row's `RowShape` (unless the catalog declares a `default`), whereas an explicit `null` literal stores `Cell::Null`.
- **Predicates (`is null` / `is not null`):** Both absent fields and explicit `Cell::Null` evaluate `field is null` to `true` and all value comparisons (`==`, `!=`, `<`, `<=`, `>`, `>=`, `in`, `~`, `has`) to `false` (SQL-like null rejection, except `not (field == x)` which negates the boolean result of `field == x`, returning `true` when `field` is null/absent!).
- **Projections (`{ a, b }` vs `without { c }`):**
  - Explicit projection `{ a, b }` materializes `Cell::Null` for requested fields absent from the input row so output rows have a uniform schema.
  - Excluding projection `without { c }` drops `c` while preserving existing row sparsity (never fabricating `Null`s).
- **Typed mapping (`#[derive(LinRow)]`):** An `Option<T>` field decodes both `Cell::Null` and an absent column as `Ok(None)`; a non-`Option` `T` field returns `Err("missing column ...")`.

### 1.2 Numeric Equality, Ordering, and `i128` Sums (`LIN-1968`)
- **Exact `i64` <-> `f64` cross-type comparison:**
  - `Int(a)` vs `Int(b)` compares in full signed 64-bit precision (values above $2^{53} = 9{,}007{,}199{,}254{,}740{,}992$ never collide or swap order).
  - `Int(i)` vs `Float(f)` compares mathematically without rounding `i` to `f64`: if `f` is outside `[i64::MIN as f64, 2^63)`, ordering is immediate; if `f` has a fractional part, `floor(f)` / `ceil(f)` determines `<` or `>` while `==` is `false`; otherwise `(f as i64)` is compared in `i128`. Index range bounds (`src/index.rs`) preserve this exact boundary.
- **Signed zero and `NaN`:**
  - `-0.0 == +0.0` is `true` and hashes identically in `GROUP BY` / `join` / `ShardRouter`.
  - In filter predicates (`==`, `<`, `>`, etc.), `Float(NaN)` evaluates to `false` (and `!=` evaluates to `true`).
  - In `sort`, `GROUP BY`, and unique index keys, `NaN` has a deterministic canonical total order (ordered after `+Infinity`, and all `NaN` bit patterns group into one bucket).
- **Aggregations (`sum`, `avg`, `window sum`):**
  - `ExactSum` ([src/exact_sum.rs](../src/exact_sum.rs)) accumulates `Cell::Int` in `i128` and `Cell::Float` with Neumaier compensated summation (`1e16 + 1.0 - 1e16 == 1.0`), converting to `f64` only when emitting the final scalar. Window integer frame sums also accumulate in `i128`.

### 1.3 Exact vs Approximate Search (`LIN-1965`)
- **Lexical (`search lex "query"`):**
  - Deterministic and **exact**. Both `FtsIndex` (`LIN\x05` v2) and query preparation use Rust's contextual Unicode `str::to_lowercase()` (so `"ΟΔΟΣ"` and `"οδος"` both normalize final sigma `ς`).
  - Scores combine substring presence (`+1` per token), whole-word token match (`+2` per alphanumeric/`_` token), and full lowered phrase match (`+3`), breaking ties by ascending document position.
- **Vector (`search vec "query"`):**
  - **Without HNSW (default):** Exact top-$k$ cosine similarity over `embedding` cells (`> 0.01` threshold). For `>= 256` rows, `FlatVecMatrix` uses SIMD `SQ8` (`vdotq_s32` on Apple Silicon) to preselect a wide candidate pool and then computes **exact `f32`/`f64` cosine** on candidates. On cold segments, `DiskEngine::search_vectors_out_of_core` scans `mmap` `.vsq8` (or `.vivf` clusters) and reranks exact `.vf32` vectors.
  - **With HNSW (`db.build_hnsw` / `db.build_hnsw_sq8`):** **Approximate** graph traversal (`ef_search`) trading strict recall on tail vectors for sub-millisecond `O(log N)` latency on large corpora.
- **Hybrid (`search "query"` / `search hybrid "query"`):**
  - Reciprocal Rank Fusion ($\text{RRF}(d) = \sum \frac{1}{60 + \text{rank}_i(d)}$) merging the lexical ranking leg and the vector ranking leg, deterministic on ties by row index / primary key.

### 1.4 CPU vs GPU Precision & Behavior (`LIN-1966`)
- **Bit-identical operations:** Integer comparisons (`i64` beyond $2^{53}$), boolean masks, UTF-8 equality/substring/`has`/`has_ci`, contextual `str::to_lowercase` lexical weights, regex NFA matching, `count by`, source-order compensated binary64 `sum` (emulated via `u32` limbs in WGSL without requiring shader `f64`), stable `sort`, RRF fusion, and BFS/DFS graph walks match CPU bit-for-bit (`tests/gpu.rs`, `tests/gpu_parity_qualification.rs`).
- **Documented floating-point boundary:** GPU vector cosine (`cosine.wgsl`) evaluates dot products and norms in `f32`, whereas the CPU reference path accumulates norms in `f64`; documents sitting within `~1e-7` of each other or of the `0.01` cosine cutoff can swap adjacent ranks.
- **Explicit resource errors:** If a query exceeds GPU buffer or workgroup dispatch limits, `GpuCompute` returns an explicit `Error` rather than silently falling back to CPU.

---

## 2. Runnable Examples (`LIN-1985`–`LIN-1988`)

### 2.1 Bulk & Transactional Ingestion (`LIN-1985`)
```rust
use lin::{Batch, Cell, Column, Db, OpenOpts, RecordBatch, SyncMode};

fn main() -> Result<(), lin::Error> {
    let mut db = Db::open_with("/tmp/lin_demo_ingest", OpenOpts {
        sync: SyncMode::Full,
        cold: true,
    })?;
    db.run("col events { id: text, service: text, latency_ms: i64, ok: bool }")?;
    db.run("index events [service, latency_ms]")?;

    // Columnar batch ingest (zero AST overhead, direct segment write when large):
    let batch = RecordBatch::new(
        vec!["id".into(), "service".into(), "latency_ms".into(), "ok".into()],
        vec![
            Column::from_cells(vec![Cell::from_text("e1"), Cell::from_text("e2")]),
            Column::from_cells(vec![Cell::from_text("auth"), Cell::from_text("billing")]),
            Column::from_cells(vec![Cell::Int(12), Cell::Int(48)]),
            Column::from_cells(vec![Cell::Bool(true), Cell::Bool(true)]),
        ],
    )?;
    db.insert_batch("events", &batch)?;

    // Atomic interactive transaction:
    let mut tx = db.transaction()?;
    tx.run(r#"update events { latency_ms: 15 } where id == "e1""#)?;
    tx.commit()?;
    Ok(())
}
```

### 2.2 Multi-Metric OLAP Analytics & Windows (`LIN-1986`)
```rust
use lin::Db;

fn main() -> Result<(), lin::Error> {
    let mut db = Db::fixture();
    db.run(r#"
        insert users [
            { id: "u1", email: "ada@example.com" },
            { id: "u2", email: "grace@example.com" }
        ]
        insert orders [
            { id: "o1", user_id: "u1", total: 120.5, ts: 1700000100000 },
            { id: "o2", user_id: "u1", total: 240.0, ts: 1700000200000 },
            { id: "o3", user_id: "u2", total: 95.0,  ts: 1700000300000 }
        ]
    "#)?;

    let summary = db.run(
        "orders | summarize { n: count, rev: sum total, p95: percentile(total, 95), buyers: dcount user_id } by bin(ts, 1h)"
    )?;
    assert_eq!(summary.rows.len(), 1);

    let ranked = db.run_batch(
        "orders | join users on user_id | window rank by user_id sort total desc as rnk | rnk == 1 | { id, users.email, total }"
    )?;
    assert_eq!(ranked.len(), 2);
    Ok(())
}
```

### 2.3 Graph Traversal & Pattern Matching (`LIN-1987`)
```rust
use lin::{Db, MatchPath, Queryable, query::pred};

fn main() -> Result<(), lin::Error> {
    let mut db = Db::fixture();
    db.run(r#"
        insert docs { id: "d1", uri: "lin://arch", title: "Architecture", layer: "spec" }
        insert docs { id: "d2", uri: "lin://wal",  title: "WAL Protocol", layer: "spec" } with edge wikilink from "d1"
        insert docs { id: "d3", uri: "lin://crc",  title: "CRC32C",       layer: "spec" } with edge wikilink from "d2"
    "#)?;

    // Compiled PointGraphPlan (< 200 ns on depth 1..3):
    let edges = db.run(r#"docs | id == "d1" | graph wikilink out depth=2 | take 20"#)?;
    assert_eq!(edges.rows.len(), 2);

    // Multi-hop pattern match projecting target fields:
    let paths = Queryable::from("docs")
        .filter(pred::eq("id", "d1"))
        .match_path(MatchPath::fwd("wikilink", "mid"))
        .select(["id", "mid.title"])
        .to_vec(&mut db)?;
    assert_eq!(paths.len(), 1);
    Ok(())
}
```

### 2.4 Lexical, Vector, and Hybrid RRF Search (`LIN-1988`)
```rust
use lin::{Db, Queryable};

fn main() -> Result<(), lin::Error> {
    let mut db = Db::fixture();
    db.run(r#"
        insert docs [
            { id: "s1", uri: "doc://1", title: "WAL Checksum", body: "CRC32C protects log frames", layer: "kb" },
            { id: "s2", uri: "doc://2", title: "Cold Segments", body: "Zstd mmap blocks and zone maps", layer: "kb" }
        ]
    "#)?;

    let lex_hits = Queryable::from("docs").search_lex("crc32c").select(["id", "title"]).take(5).to_vec(&mut db)?;
    let hybrid_hits = db.run(r#"docs | search hybrid "log frame checksum" | { id, title } | take 5"#)?;
    assert_eq!(lex_hits[0].get_str("id").and_then(|c| c.text()), Some("s1"));
    assert!(!hybrid_hits.rows.is_empty());
    Ok(())
}
```
