# Lin

Lin — язык своей локальной БД: пайпы, типизированный каталог, два мира записи (`append` / reducer), свой план. Не SQL и не Kusto.

**0.4** — встраиваемая локальная БД: durable `--data`, backup, multi-reader, compaction, flock, quotas, `SyncMode`, cold/mmap, WAL shipping, durable follower, **stable** Queryable/cursor/typed/async, **FTS postings** (`FtsSeek`) + lazy hop/search. Не multi-writer.

Сейчас есть парсер, typecheck, IR плана, in-memory store (один reducer, один `gen`), исполнитель, WAL+snapshot с compaction, `backup` CLI, durable follower, **живой search lex/vec/hybrid** (FTS postings + hashing-эмбеддер по `embed_id`). DuckDB/WASM backends и multi-writer в этом milestone нет.

## Стабильный API (0.3+)

Публичный контракт: `Db::{empty,fixture,open,open_with,open_read,open_follower,open_follower_with,bootstrap_follower,close,checkpoint,run,run_group,prepare,explain_as,reader,export_backup,import_backup,import_backup_into,export_wal_since,apply_wal,stats,with_quotas,with_sync_mode,with_embedder}`, `ReadDb::{run,prepare,stats,clone}`, `Quotas`, `SyncMode`, `OpenOpts`, `Embedder` / `HashingEmbedder`, плюс `parse` / `compile` / `run` / `explain*`, типы `Handle` / `Done` / `Prepared` / `Error` / `Row` / `ProjectedRow` / `Cell` / `Store` / `Plan` / `Stats` / `ReopenPhases` / `VERSION`.

**Также stable:** `Queryable` / `BoundQueryable` / `query::pred`, `QueryCursor`, `FromRow` / `LinRow` / `FromCell` / `map_rows` / `cell_get`, feature `async` (default-on): `AsyncDb` / `AsyncReadDb` / `to_vec_async` / `stream_*` (`spawn_blocking`, не async storage).

### Queryable + typed + async

```rust
use lin::query::pred;
use lin::{Db, MatchPath, Queryable};

let mut db = Db::fixture();

// explain without string DSL
let plan = Queryable::from("docs")
    .filter(pred::eq("wing", "rag"))
    .take(5)
    .explain(&mut db)?;

// Keyset paging — prefer over deep skip (field must allow `>`: num/time, not text id)
let page1 = db.from("orders").select(["id", "total"]).sort("total", false).take(20).to_vec()?;
let last = page1.last().unwrap().get("total").and_then(|c| c.as_f64()).unwrap();
let page2 = db.from("orders").after("total", last).sort("total", false).take(20).to_vec()?;

// Lazy cursor: filter|project|skip|take, FK join, hop d=1, search lex|hybrid|take
let mut cur = Queryable::from("orders")
    .filter(pred::gt("total", 100.0))
    .join("users", "user_id")
    .select(["id", "users.email", "total"])
    .take(10)
    .cursor(&db)?;
assert!(cur.is_lazy());
while let Some(row) = cur.next_projected() {
    let row = row?; // shared field schema + Vec<Cell>, no BTreeMap per row
    let _email = row.get("users.email");
}

// hop depth=1 is lazy; match / hop depth>1 materialize
let _ = Queryable::from("docs")
    .filter(pred::eq("id", id))
    .hop("wikilink")
    .select(["id", "title"])
    .to_vec(&mut db)?;
let _ = Queryable::from("docs")
    .filter(pred::eq("id", id))
    .match_path(MatchPath::fwd("wikilink", "b"))
    .select(["id", "b.title"])
    .to_vec(&mut db)?;

// search: hybrid (default), lex (FTS), or vec
let _ = Queryable::from("docs").search_lex("wal").take(5).cursor(&db)?;
let _ = Queryable::from("docs").search_vec("wal shipping").take(5).to_vec(&mut db)?;
```

**Lazy cursor:** `filter` / `project` / `skip` / `take`, один `join`/`left_join` по FK, **`hop` depth=1**, **`search lex|hybrid` + take** (FTS idxs). Для проекций `next_projected()` возвращает compact `ProjectedRow`; обычный `Iterator<Item=Row>` сохранён совместимым. `graph` / `match` / `sort` / `union` / `search vec` / hop depth>1 — buffered. Deep `skip` дороже keyset (`.after` + `take`).

**Experimental (вне freeze):** `ship` (TCP WAL, без TLS), `RecordBatch` / `run_batch`, `Db::run_stmt`, `RowExt`. См. [CHANGELOG](CHANGELOG.md).

**OLAP рядом с row-API:** `Db::run` / `Prepared::run` → `Vec<Row>`; `run_batch` → [`RecordBatch`] (колонки, experimental). Columnar: `filter?|project|take` и hot join `orders ⋈ users` (SoA); иначе fallback в row-maps.

Features: `derive`, `async` — в `default` (часть 0.3 контракта). Opt-in neural: `embed-ollama` (`OllamaEmbedder`), `embed-onnx` (`OnnxEmbedder`) — **не** default; hashing остаётся.

`Db::reader()` — in-process снимок текущего `gen`. `open_read` — shared **FENCE** (можно рядом с writer; checkpoint ждёт readers). `export_wal_since` / `apply_wal` — ship WAL; **`apply_wal` на primary durable запрещён**; на **follower** (`open_follower` / `bootstrap_follower`) пишет frames в log и применяет (hot standby). In-memory `apply_wal` как раньше. `pin`/`unpin` — memory-pins. Snapshot: `LIN\x04` MessagePack self-contained; `cold/*.bin` — lazy mmap page-in.

`Db::run_group([...])` исполняет независимые snippets атомарно и в `SyncMode::Full` делает один WAL frame + один durability flush на группу. Ошибка откатывает всю группу, включая live indexes и FTS.

## Local-prod guarantees

| Есть | Нет |
|---|---|
| Crash после успешного commit (`SyncMode::Full`) → log/snapshot | Multi-writer / сеть / полный MVCC |
| Exclusive `LOCK` (writer↔writer) | ANN indexes (hashing vec есть) |
| Shared `FENCE` — `open_read`∥writer; checkpoint ждёт readers | |
| Durable follower: `bootstrap_follower` + `apply_wal` | |
| Cold lazy mmap page-in | |
| FTS postings (`FtsSeek`, rebuild-on-open) | |
| Quotas / `SyncMode::Normal` (opt-in) / WAL ship / pins | |

Память = snapshot + lazy cold page-in + WAL tail. Durable `Db` на `Drop` — best-effort checkpoint.

## Лаконичный диалект

После `|` голый предикат — это `where`. В запросе `{ id, title }` — проекция. `where` и `pick` остаются синонимами.

```
docs | wing == "rag" and ts > ago 7d | { id, title, room }
docs | id == "e7c98d54-b4d6-4165-86e9-9b999e7ce9c3"
docs | title has "wal"
docs | title ~ "wal"
docs | title ~ "wal" | count
docs | title ~ /wal.*/i
orders | total > 100 | join users on user_id | { id, users.email, total }
docs | hop wikilink | { id, title }
docs | id == "…" | graph wikilink depth=2 | { rel, from, to }
docs | id == "…" | match -wikilink-> b | { id, b.title }
docs | id == "…" | match a -wikilink-> b -wikilink-> c | { a.title, b.title, c.title }
docs | id == "…" | match -wikilink*1..2-> b | { b.title }
docs | id == "…" | match <-wikilink- src | { src.title }
docs | id == "…" | match -[e:wikilink]-> b | { e.from, e.to, b.title }
docs | search "wal" | take 20
docs | wing == "rag" | { id, title } | skip 40 | take 20
docs | { id } | union orders | { id }
let x = docs | wing == "rag" | { id }
x | take 5
append facts { s: "lin", p: tagged, o: "db" }
insert docs [
  { uri: "raw://a", title: "A", layer: "wiki" },
  { uri: "raw://b", title: "B", layer: "wiki" }
]
append facts [
  { s: "a", p: tagged, o: "x" },
  { s: "b", p: tagged, o: "y" }
]
append edges [
  wikilink "a" -> "b",
  wikilink "b" -> "c"
]
update docs[wing == "rag"] cas each { room: "inbox" }
delete docs[wing == "old"] cas each
index docs [wing, ts]
index orders [user_id, ts]
index docs unique [uri]
update docs[id == "…"] cas "sha256:…" { room: "inbox" }
delete docs[id == "…"] cas "sha256:…"
delete edge wikilink "a" -> "b"
delete facts[s == "lin" and p == tagged and o == "db"]
col notes { title: text }
rel cites
```

`union` — совместимые столбцы по имени после последней проекции каждой стороны. `hop` — узлы по ребру (depth 1..=3). `graph` — те же обход и лимиты, но строки рёбер `{ rel, from, to }` (после шага scope = edges). `match` — path pattern: `-rel-> bind`, `<-rel- bind`, `-rel*1..3-> bind`, `-[e:rel]-> bind` (до 3 hops, depth ≤3; edge bind только при depth 1). Узлы как `bind.field`, ребро как `e.rel`/`from`/`to`. `let` — только чтение, не запись. Несколько `append`/`insert`/`delete`/`update` в одной строке `run()` — один пакет, один `gen` (или ничего); запросы после записи видят новое состояние; любая ошибка откатывает всё. Список в `insert`/`append` — один `InsertPack`/`Append`, один `gen`. `cas each` читает hash каждой строки на старте пакета и CAS-ит все; смена mid-pack откатывает всё. `update`/`delete` без `cas` / `cas each` — ошибка. Пустой `[]` — ошибка. `with edge` на списке insert запрещён. `col` / `rel` / `index` создают живую коллекцию/ребро/индекс в store.

Составной индекс: порядок полей = leftmost prefix. `wing == "rag" and ts > ago 7d` по `[wing,ts]` — `IndexSeek index=docs[wing,ts]` (равенство слева + range на следующем). Только `wing ==` — тот же индекс. Только `ts >` — scan, leftmost не закрыт. `id ==` остаётся `Get`. `wing == "rag" or wing == "sys"` — несколько seek по индексу и объединение; если хоть одна ветка `or` не индексируется — scan.

`has` — целое слово (граница токена). `~ "…"` — подстрока. `~ /…/i` — регулярка (литерал проверяется при компиляции). `ago 7d` = `now - 7d` (единица обязательна).

Запись — отдельный statement, не хвост пайпа. Неизвестный столбец, hop без `rel`, join без `fk`, `update` без `cas` — ошибка компиляции.

Неявный `take 50`. `skip N` / `offset N` — отбросить первые N строк (пейджинг: `skip 40 | take 20`). `search` / `search hybrid` — RRF(lex + vec): lex через **FTS postings** (`FtsSeek`, поля с `fts` в каталоге), vec через локальный hashing-эмбеддер (`embed_id`, по умолчанию dim из суффикса `/768`); не нейросеть. `search lex` — только FTS+residual score. `search vec` — cosine по полю `embedding` (пишется на insert / `reembed`). Подмена: `Db::with_embedder`.

## Roadmap

Кратко ниже; полный план — canvas / wiki `lin-roadmap`.

| Фаза | Статус |
|---|---|
| P0–P2: 0.2 + backup/FENCE/cold/SyncMode | **готово** |
| P3 hot standby | **готово** (`open_follower` / `apply_wal`) |
| P3c live vec/hybrid | **готово** (hashing embedder + RRF; neural → `with_embedder`) |
| **0.2.x ship + ops** | **готово** (критерии met: n1–n4 · tests · README recipe) |
| **0.3** | **готово** — Queryable/cursor/FromRow/async в stable + CHANGELOG |
| **0.3.x** | **готово** — m1 Neural embed features · m4 filter+project `run_batch` · join SoA |
| **0.4** | **готово** — m2 FTS postings / `FtsSeek` · m5 lazy hop d=1 + search lex\|hybrid |
| **0.5** | m3 ANN · m6 wasm32 · o1 TLS/auth/fanout (после скучного ship) |
| **Не 0.x** | Multi-writer / полный MVCC / consensus · DuckDB как storage backend |

Подробный план (чеклисты, файлы, риски): см. wiki `lin-roadmap` / canvas.

## Hot standby (recipe)

Primary держит writer lock на `--data` (в т.ч. пока крутится `wal-serve`). После checkpoint log пуст — bootstrap с backup, sync догоняет только хвост WAL.

```bash
# 1) primary: данные + portable backup
lin --data primary run 'insert docs { uri: "raw://a", title: "A", layer: "wiki" }'
lin --data primary backup export /tmp/boot.linbak

# 2) follower из backup
lin --data follower follower bootstrap --backup /tmp/boot.linbak

# 3) primary: ещё запись (не закрывать / не checkpoint'ить зря перед serve)
lin --data primary run 'insert docs { uri: "raw://b", title: "B", layer: "wiki" }'

# 4) в одном терминале — отдать WAL (держит writer lock)
lin --data primary wal-serve --listen 127.0.0.1:9876

# 5) в другом — one-shot sync (или --loop 2)
lin --data follower follower sync --from 127.0.0.1:9876
lin --data follower follower status
lin --data follower --follower run 'docs | take 10'
```

Low-level: `lin --data follower --follower apply-wal frames.bin`. API: `lin::ship::{pull,serve_blocking}`.

## Бенчмарки (airbug-bench): Lin vs SQLite vs DuckDB vs Postgres vs MySQL

Сравнительные hot paths в `benches/compare.rs` через [airbug](https://github.com/themoretheless/airbug) (`airbug` + `airbug-bench` @ git `release`, `Suite`, `harness = false`).

```bash
# список кейсов
cargo bench --bench compare -- --list

# быстрый прогон (нужен --release; airbug-bench отказывается от debug)
cargo bench --bench compare -- --profile quick

# как в CI: без durable/cold/wal + markdown + .airbug-bench/ci/
# после прогона печатает ссылку на airbug dash (http://127.0.0.1:8790/)
./scripts/ci-bench.sh

# только point get / insert / join / append_log / FTS / фазовый профиль
cargo bench --bench compare -- --filter point_get
cargo bench --bench compare -- --filter insert_bulk_1k --samples 8
cargo bench --bench compare -- --filter join
cargo bench --bench compare -- --profile quick --filter append_log
cargo bench --bench compare -- --profile thorough --tag fts
cargo bench --bench compare -- --profile thorough --tag phase
```

### CI

Workflow [`.github/workflows/bench.yml`](.github/workflows/bench.yml) на `push`/`pull_request` → `main`: `--profile quick`, без Docker. В отчёте Lin / SQLite / DuckDB / HashMap; Postgres и MySQL пропускаются без серверов. FTS query cases включены; диагностические `*phase*` и шумные durable/cold/wal/reopen cases исключены. После прогона `scripts/check-bench-budget.py` сравнивает Lin median с [`benches/ci-baseline.json`](benches/ci-baseline.json): **warn** при >1.5×, **fail** при >3× на `point_get` / `filter_eq` / `join_inner`. Иначе job падает только при ошибке compile/harness.

Где смотреть: **Actions → bench → Job summary** (markdown-таблица) и artifact **`bench-report`** (`run.json` + `report.html` + `bench-report.md`). Локально: `./scripts/ci-bench.sh` сразу печатает (и при живом hub открывает) **airbug dash** `http://127.0.0.1:8790/`, затем гоняет бенчи в `.airbug-bench/ci/` (hub: `cargo run -p airbug-hub -- serve --root <lin>`).

Движки: **Lin**, **SQLite** (`rusqlite` bundled), **DuckDB** (bundled; собирается на mac aarch64), **Postgres** / **MySQL** (опционально, через URL), плюс **HashMap** только для point get. N=10 000 для тёплых чтений (fixture; setup вне тайминга). Bulk insert: схема/индекс в setup, в тайминге только запись.

Сравнимо: point get по id, `wing ==`, range `wing`+`ts`, substring (`title ~ "wal" | count` ≈ `COUNT(*) … LIKE '%wal%'`), materialize `SELECT id,title`, **join** (10k `orders` ⋈ 1k `users` по FK; Lin `run_batch` / SoA+`RecordBatch` и lazy cursor vs SQL `INNER JOIN`; плюс `total > 100` затем join), bulk insert 1k/10k, **append_log** (`append facts` vs `INSERT INTO logs`) 1k/10k.
Join: row-API (`run` → `Vec<Row>`) и OLAP-путь (`run_batch` → `RecordBatch`) рядом; бенч join меряет batch. DuckDB — референс columnar OLAP.  

Lin-only diagnostics:
- **FTS:** `fts_lex_selective`, `fts_lex_common`, `fts_lex_miss`, `fts_hybrid_common`, плюс `reopen_phase_5k/rebuild_fts`. Lex `take` использует bounded top-k heap и клонирует только результат; SQL `LIKE` остаётся отдельным сравнительным shape.
- **Insert phases:** обычный insert, `lin_no_embed`, `lin_no_embed_no_scalar_index` и `lin_no_embed_no_scalar_no_fts`; последовательные разницы оценивают цену hashing embedder, scalar-index и FTS maintenance.
- **Cursor phases:** `lin_cursor_open`, `lin_cursor_scan_project`, `lin_cursor_scan_projected_row`, обычный и compact lazy join cursor. Разницы отделяют setup, `Row=BTreeMap` materialization и join probe/emission.
- **Reopen phases:** полный hot/cold open рядом с отдельными rebuild row maps / scalar indexes / FTS на 5k строк. `Stats::reopen` даёт real-open breakdown: setup/lock, snapshot decode, WAL replay, metadata/head, indexes, row maps и FTS.
- **Group commit:** `group_commit_16/lin_sequential_full` против `lin_grouped_full` — одинаковые 16 уникальных append statements, 16 flush против одного.

Не сравниваем здесь (и не подтасовываем): Lin `hop`/`match`, neural vec, CAS — отдельный слой. Фазовые цифры являются диагностическими разностями медиан, не additive tracing: их нельзя механически суммировать из-за cache state и allocator noise.

### Postgres / MySQL

Без сервера кейсы пропускаются (Lin/SQLite/DuckDB всё равно бегут). URL: `LIN_BENCH_PG_URL` / `LIN_BENCH_MYSQL_URL`, иначе авто-probe локальных портов.

```bash
# рекомендуемый стек (в корне lin)
docker compose -f docker-compose.bench.yml up -d
# либо явно:
export LIN_BENCH_PG_URL='postgresql://lin:lin@127.0.0.1:55432/lin'
export LIN_BENCH_MYSQL_URL='mysql://lin:lin@127.0.0.1:53306/lin'
```

Альтернативы из `Documents/Sources`:
- MySQL smoke из **dbill**: `docker compose -f ../dbill/docker-compose.yml up -d mysql` → `mysql://dbill:dbill@127.0.0.1:33306/dbill_smoke`
- Postgres через Homebrew: задача **ppduster** `macos-stack-postgres` (`postgresql@17`); после старта сервиса probe `postgresql://postgres@127.0.0.1:5432/postgres`

Dev-deps: `airbug` (unit) и `airbug-bench` (бенчи) с ветки `release`; плюс `postgres` / `mysql`.

## Запуск

```bash
cargo test
cargo build

# план (дерево)
cargo run -- explain 'docs | wing == "rag" | search "wal" | { id, title } | explain cost'

# выполнить на fixture (эфемерная память, без --data)
cargo run -- run 'docs | wing == "rag" | { id, title }'
cargo run -- run --explain 'docs | search "wal" | take 5'

# граф плана
cargo run -- explain --graph mermaid 'docs | wing == "rag" | search "wal" | { id, title }'
cargo run -- explain --graph dot 'docs | wing == "rag" | search "wal" | { id, title }'

# то же с диском (переживает рестарт процесса)
cargo run -- --data .lin run 'insert docs { uri: "raw://n/p", title: "hi", layer: "wiki" }'
cargo run -- --data .lin run 'docs | uri == "raw://n/p" | { id, title }'
cargo run -- --data .lin run --file batch.lin
cargo run -- run - < batch.lin
```

То же в языке: `| explain graph` (Mermaid) и `| explain dot` (Graphviz). `| explain run` после исполнения пишет фактические `rows`/`ms`.

CLI: `lin run '<запрос>'` печатает строки и `gen`. `lin run --file batch.lin` и `lin run -` читают программу из файла / stdin. `lin explain '<запрос>'` печатает план или ошибку компиляции. `lin --data .lin run '…'` пишет в каталог данных. Hot standby: `wal-serve` + `follower sync` (см. recipe выше); low-level `apply-wal`. `lin backup export|import`, `lin stats`, `lin version`.

## Backup

```bash
lin --data .lin backup export /tmp/lin-bak.json
lin backup import /tmp/lin-bak.json --data .lin2
lin --data .lin2 stats
```

Формат backup — `LIN\x04` + MessagePack (self-contained; legacy JSON backup ещё читается). Export перед записью делает checkpoint, если store durable.

## Данные на диске

Без `--data` store эфемерный (fixture в памяти) — так живут текущие тесты языка и исполнителя.

`--data <dir>` открывает durable store: exclusive flock → snapshot (`LIN\x04` MessagePack, legacy JSON) + replay tail → RAM. `lin stats` печатает все `reopen_*_ms` фазы. Запись: flush при `Full` / на checkpoint при `Normal`; `run_group` объединяет атомарную группу в один flush. Checkpoint пишет self-contained snapshot (+ опционально `cold/*.bin` cache) и **обнуляет log**.

```
.lin/
  LOCK       advisory flock (writer exclusive / open_read shared)
  head       JSON: gen, catalog_hash, embed_id
  log        append-only WAL (compacted on checkpoint):
             `LIN\x02` raw columnar / `LIN\x01` MessagePack / legacy JSON
  snapshot   `LIN\x04` MessagePack(Snapshot), self-contained (legacy JSON reads)
  cold/      optional mmap cache of large collections (rows also inlined in snapshot)
```

Пакет лога: `{"gen":N,"next_id":N,"pack":{"type":"insert"|"insert_bulk"|"append_fact"|"append_facts_bulk"|"append_edge"|"append_edges_bulk"|…}}`. Ячейки: `{"t":"Text","v":"…"}`. Bulk-формы пишут один record вместо Batch-of-N. Обрезанная последняя запись лога игнорируется; при открытии лог обрезается до последнего целого фрейма. `insert … with edge` кладёт рёбра в тот же пакет. Несколько записей в одном `run()` или bulk-список — один `batch` / `*_bulk`. Индекс живёт в `schema_index` + snapshot `extra_indexes`; при open пересобирается.

## Как смотреть граф

**Mermaid** — вставить блок в GitHub / Notion / любой рендер Markdown:

````markdown
```mermaid
flowchart TD
  n0["Reduce Read"]:::reduce
  n1["Take 50 implicit"]:::read
  n0 --> n1
```
````

**DOT** — в SVG:

```bash
lin explain --graph dot 'docs | search "wal"' | dot -Tsvg > plan.svg
```

Узлы — IR плана, рёбра — поток данных. Цвет/форма по эффекту: Read (синий), Append/запись (оранжевый), Reduce (фиолетовый).
