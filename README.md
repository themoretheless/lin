# Lin

Lin — язык своей локальной БД: пайпы, типизированный каталог, два мира записи (`append` / reducer), свой план. Не SQL и не Kusto.

**0.4** — встраиваемая локальная БД: durable `--data`, backup, multi-reader, compaction, flock, quotas, `SyncMode`, cold/mmap, WAL shipping, durable follower, **stable** Queryable/cursor/typed/async, **FTS postings** (`FtsSeek`) + lazy hop/search. Не multi-writer.

Сейчас есть парсер, typecheck, IR плана, in-memory store (один reducer, один `gen`), исполнитель, WAL+snapshot с compaction, `backup` CLI, durable follower, **живой search lex/vec/hybrid** (FTS postings + hashing-эмбеддер по `embed_id`). DuckDB/WASM backends и multi-writer в этом milestone нет.

## Стабильный API (0.3+)

Публичный контракт: `Db::{empty,fixture,open,open_with,open_read,open_follower,open_follower_with,bootstrap_follower,close,checkpoint,run,run_group,prepare,explain_as,reader,export_backup,import_backup,import_backup_into,export_wal_since,apply_wal,stats,with_quotas,with_sync_mode,with_embedder}`, `ReadDb::{run,prepare,stats,clone}`, `Quotas`, `SyncMode`, `OpenOpts`, `Embedder` / `HashingEmbedder`, плюс `parse` / `compile` / `run` / `explain*`, типы `Handle` / `Done` / `Prepared` / `Error` / `Row` / `ProjectedRow` / `Cell` / `Store` / `Plan` / `Stats` / `ReopenPhases` / `VERSION`.

**Также stable:** `Queryable` / `BoundQueryable` / `query::pred`, `QueryCursor`, `FromRow` / `LinRow` / `FromCell` / `ToCell` / `map_rows` / `cell_get` / `cell_opt`, `Db::execute` / `scalar`, `Queryable::{first,first_or,first_typed,single,scalar,cursor,buffered}`, `QueryCursor::next_typed`, feature `async` (default-on): `AsyncDb` / `AsyncReadDb` / `to_vec_async` / `stream_*` (`spawn_blocking`, не async storage).

### Queryable + typed + async

```rust
use lin::query::pred;
use lin::{Db, LinRow, MatchPath, Queryable};

let mut db = Db::fixture();

// explain without string DSL
let plan = Queryable::from("docs")
    .filter(pred::eq("wing", "rag"))
    .take(5)
    .explain(&mut db)?;

let _one = db.from("docs").filter("wing == \"rag\"").select(["id"]).first()?;
let _hits: i64 = db.from("docs").count().scalar_as()?;

#[derive(LinRow)]
#[lin(collection = "docs")]
struct DocTitle { id: String, title: String }
let _page: Vec<DocTitle> = db.from_typed::<DocTitle>()?.take(5).to_vec_typed()?;
let _wal = db
    .from_typed::<DocTitle>()?
    .filter(|d| d.title.has("WAL"))
    .take(5)
    .to_vec_typed()?;
let mut cur = db.from_typed::<DocTitle>()?.take(5).cursor()?;
let _ = cur.next_typed::<DocTitle>();

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

**Experimental (вне freeze):** `ship` (WAL `LIN\x06`, token, optional TLS, one sink), `RecordBatch` / `run_batch`, `Db::run_stmt`, `RowExt`. См. [CHANGELOG](CHANGELOG.md).

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
| FTS postings (`FtsSeek`, checkpoint blob + validated rebuild fallback) | |
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
rel cites
owned stamp { hash: text, ts: time }
col notes { title: text, stamp }
filter docs layer == "wiki"
docs all | take 5
unfilter docs
```

`union` — совместимые столбцы по имени после последней проекции каждой стороны. `hop` — узлы по ребру (depth 1..=3). `graph` — те же обход и лимиты, но строки рёбер `{ rel, from, to }` (после шага scope = edges). `match` — path pattern: `-rel-> bind`, `<-rel- bind`, `-rel*1..3-> bind`, `-[e:rel]-> bind` (до 3 hops, depth ≤3; edge bind только при depth 1). Узлы как `bind.field`, ребро как `e.rel`/`from`/`to`. `let` — только чтение, не запись. Несколько `append`/`insert`/`delete`/`update` в одной строке `run()` — один пакет, один `gen` (или ничего); запросы после записи видят новое состояние; любая ошибка откатывает всё. Список в `insert`/`append` — один `InsertPack`/`Append`, один `gen`. `cas each` читает hash каждой строки на старте пакета и CAS-ит все; смена mid-pack откатывает всё. `update`/`delete` без `cas` / `cas each` — ошибка. Пустой `[]` — ошибка. `with edge` на списке insert запрещён. `col` / `rel` / `index` создают живую коллекцию/ребро/индекс в store. `owned stamp { hash, ts }` — value object: поля впечатываются в коллекцию, отдельной таблицы нет. `filter docs pred` — каталожный предикат на **чтение** (план видит первый `Filter`); `docs all` / `Queryable::ignore_filters` его обходят; `unfilter` снимает. Запись (`update`/`delete` по pred) фильтр не прячет. Связи — объявленные `rel`/`fk` и явный `join`/`hop`/`match`, не Include. Чтение — snapshot/`ReadDb` без трекера; запись — `update … cas` / один `run()` = один `gen`.

Составной индекс: порядок полей = leftmost prefix. `wing == "rag" and ts > ago 7d` по `[wing,ts]` — `IndexSeek index=docs[wing,ts]` (равенство слева + range на следующем). Только `wing ==` — тот же индекс. Только `ts >` — scan, leftmost не закрыт. `id ==` остаётся `Get`. `wing == "rag" or wing == "sys"` — несколько seek по индексу и объединение; если хоть одна ветка `or` не индексируется — scan.

`has` — целое слово (граница токена). `~ "…"` — подстрока. `~ /…/i` — регулярка (литерал проверяется при компиляции). `ago 7d` = `now - 7d` (единица обязательна). `timestamp(1700000000123)` — абсолютное Unix-время в миллисекундах, тип `time`; принимает знаковое i64, включая даты до эпохи.

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
| **0.5** | m3 ANN · m6 wasm32 · o1 fanout (TLS/token/one-sink — в Unreleased) |
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
# localhost: plaintext ok. Другая машина: --tls-cert/--tls-key --token
lin --data primary wal-serve --listen 127.0.0.1:9876 --token lab

# 5) в другом — one-shot sync (или --loop 2)
lin --data follower follower sync --from 127.0.0.1:9876 --token lab
lin --data follower follower status
lin --data follower --follower run 'docs | take 10'
```

Low-level: `lin --data follower --follower apply-wal frames.bin`. API: `lin::ship::{pull, pull_with, serve_blocking_opts}`. Token also via `LIN_SHIP_TOKEN`. TLS: `--tls-ca` on the follower (or `--tls-insecure` only in lab).

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

Postgres bulk insert проверяет все шесть записанных полей отдельным подключением после commit, вне таймера. Помимо построчного INSERT в транзакции есть `insert_native_1k/postgres_copy` и `insert_native_10k/postgres_copy`: binary COPY с теми же индексами, проверкой числа строк и полным readback.

Для проверки затрат нормализации FTS на bulk insert можно задать `LIN_BENCH_MIXED_CASE=1` и выбрать `--filter insert_bulk`: заголовки fixture будут со смешанным регистром у всех движков. Этот режим предназначен для вставок; обычные read cases используют условия для стандартных строчных заголовков.

Сравнимо: point get по id, `wing ==`, range `wing`+`ts`, substring (`title ~ "wal" | count` ≈ `COUNT(*) … LIKE '%wal%'`), materialize `SELECT id,title`, **join** (10k `orders` ⋈ 1k `users` по FK; Lin `run_batch` / SoA+`RecordBatch` и lazy cursor vs SQL `INNER JOIN`; плюс `total > 100` затем join), bulk insert 1k/10k, **append_log** (`append facts` vs `INSERT INTO logs`) 1k/10k.
Join: row-API (`run` → `Vec<Row>`) и OLAP-путь (`run_batch` → `RecordBatch`) рядом; бенч join меряет batch. DuckDB — референс columnar OLAP.  

Lin-only diagnostics:
- **FTS:** `fts_lex_selective`, `fts_lex_common`, `fts_lex_miss`, `fts_hybrid_common`, плюс `reopen_phase_5k/rebuild_fts`. Lex `take` использует bounded top-k heap и клонирует только результат; SQL `LIKE` остаётся отдельным сравнительным shape.
- **Insert phases:** обычный insert, `lin_no_embed`, `lin_no_embed_no_scalar_index` и `lin_no_embed_no_scalar_no_fts`; последовательные разницы оценивают цену hashing embedder, scalar-index и FTS maintenance.
- **Cursor phases:** `lin_cursor_open`, `lin_cursor_scan_project`, `lin_cursor_scan_projected_row`, обычный и compact lazy join cursor. Разницы отделяют setup, `Row=BTreeMap` materialization и join probe/emission.
- **Reopen phases:** полный hot/cold open рядом с отдельными rebuild row maps / scalar indexes / FTS на 5k строк. `Stats::reopen` даёт real-open breakdown: setup/lock, snapshot decode, WAL replay, metadata/head, indexes, row maps и FTS.
- **Group commit:** `group_commit_16/lin_sequential_full` против `lin_grouped_full` — одинаковые 16 уникальных append statements, 16 flush против одного.

Не сравниваем здесь (и не подтасовываем): Lin `hop`/`match`, neural vec, CAS — отдельный слой. Фазовые цифры являются диагностическими разностями медиан, не additive tracing: их нельзя механически суммировать из-за cache state и allocator noise.

Таргетированный `thorough` прогон 2026-09-17 (один процесс, локальная машина): FTS common top-20 `568 → 182 µs`; insert 10k full `27.38 → 20.47 ms`; compact scan/project `1.68 → 0.90 ms`; compact SoA join cursor `1.70 → 0.48 ms`; hot reopen 5k `158.6 → 82.0 ms`; 16 уникальных Full commits `5.05 ms` sequential против `0.83 ms` grouped. Это benchmark evidence, не переносимый SLA.

### Расширенное сравнение и контракт

[`docs/benchmark-contract.md`](docs/benchmark-contract.md) фиксирует условия измерений.
`scripts/bench-peers.py` сравнивает проверяемые результаты Lin с SQLite, DuckDB,
PostgreSQL, MySQL, MongoDB, SQL Server, Kusto и pandas. Недоступные движки дают
неполный отчёт и ненулевой exit code; `--require-wins` дополнительно проверяет
победу Lin в каждом измеренном сценарии. Rust row API и Python driver/DataFrame
API измеряются отдельно от существующего native Rust harness.

```sh
python3 -m venv .bench-venv
.bench-venv/bin/python -m pip install -r scripts/bench-peers-requirements.lock
cargo build --release --example peer_bench
.bench-venv/bin/python scripts/bench-peers.py --engines lin sqlite duckdb postgres mysql mongo pandas --repeats 3 --require-wins --output .airbug-bench/peers
```

Update/delete: подготовка вне таймера, read-back validation, 1k/10k/100k строк.
`--max-iterations 1 --warmup-ms 0` ограничивает дорогие fresh-input прогоны.

### Postgres / MySQL

Postgres fixtures создают отдельную схему `lin_bench_*` для каждого подключения и удаляют её при завершении. Таблицы остаются обычными WAL-logged таблицами; schema setup и cleanup находятся вне таймера. Пользователю подключения нужен `CREATE` на тестовой базе. Для отдельного прогона PostgreSQL установите `LIN_BENCH_SKIP_MYSQL=1`: это отключает подключение и setup MySQL.

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

`--data <dir>` открывает durable store: exclusive flock → snapshot (`LIN\x04` MessagePack, legacy JSON) + optional `fts/*.bin` + replay tail → RAM. `lin stats` печатает все `reopen_*_ms` фазы. Запись: flush при `Full` / на checkpoint при `Normal`; `run_group` объединяет атомарную группу в один flush. Checkpoint пишет self-contained snapshot (+ `fts/*.bin` postings, опционально `cold/*.bin` cache) и **обнуляет log**.

```
.lin/
  LOCK       advisory flock (writer exclusive / open_read shared)
  head       JSON: gen, catalog_hash, embed_id
  log        append-only WAL (compacted on checkpoint):
             `LIN\x06` CRC32 envelope (columnar / MessagePack); reads `LIN\x02` / `LIN\x01` / legacy JSON
  snapshot   `LIN\x04` MessagePack(Snapshot), self-contained (legacy JSON reads)
  fts/       posting lists (`LIN\x05`) per FTS collection; rebuild if missing/stale
  cold/      optional mmap cache of large collections (rows also inlined in snapshot)
```

Пакет лога: `{"gen":N,"next_id":N,"pack":{"type":"insert"|"insert_bulk"|"append_fact"|"append_facts_bulk"|"append_edge"|"append_edges_bulk"|…}}`. Ячейки: `{"t":"Text","v":"…"}`. Bulk-формы пишут один record вместо Batch-of-N. Обрезанная последняя запись лога игнорируется; при открытии лог обрезается до последнего целого фрейма. `insert … with edge` кладёт рёбра в тот же пакет. Несколько записей в одном `run()` или bulk-список — один `batch` / `*_bulk`. Индекс живёт в `schema_index` + snapshot `extra_indexes`; при open пересобирается. FTS postings — `fts/*.bin` на checkpoint; несовпадение gen/nrows/fields → rebuild.

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

Новые WAL-фреймы используют `LIN\x06` с CRC32 по codec/payload. Чтение прежних форматов сохранено; старые бинарники/replication followers не читают новый envelope и должны обновляться вместе с writer. Snapshot и backup остаются `LIN\x04`. Codec 3 внутри `LIN\x06` компактно записывает разреженные embedding-векторы с точным сохранением битов, включая `-0.0` и NaN. Плотные/малые векторы сохраняют codec 2. Перед использованием codec 3 обновите все readers/followers; прежние форматы продолжают читаться. Размер кадра по-прежнему ограничен 16 MiB, сумма декодированных sparse-векторов и их контейнеров — 64 MiB.

## Experimental GPU compute

Покрытие API, операции и результаты проверки: [compute support](docs/compute-support.md).

`cargo run --release --features gpu --example gpu_check` проверяет настоящий
GPU: cosine против CPU, vec/hybrid и reader; печатает адаптер, погрешность и
время с передачей данных. Программный адаптер отвергается.

```rust,ignore
let gpu = std::sync::Arc::new(lin::gpu::GpuCompute::new()?);
let mut db = lin::Db::fixture().with_gpu(gpu);
let rows = db.run("docs | search vec \"wal\" | take 20")?.rows;
```

Feature `gpu` включает wgpu, пользовательский WGSL compute API и шейдеры
фильтрации, regex, агрегатов, joins, сортировки, поиска и обхода графа.
GPU-бэкенд сохраняется в `Db::reader()`. Размер пакетов ограничен лимитами
устройства. Инициализация и ошибки
readback возвращаются явно; без `with_gpu` остаётся CPU. Это native API,
блокирующий до завершения GPU; из async вызывается через существующий
`spawn_blocking`.

GPU накапливает в f32, CPU — в f64: почти равные scores могут поменять порядок
или перейти порог 0.01. GPU принимает только конечные компоненты. Executor хранит последний корпус
векторов на GPU, загружая его заново при смене embedding, их порядка или
размерности запроса. Проверка Arc-владельцев корректна и при прямой мутации
публичного Store; gen не служит единственным признаком изменения.
Упаковка и загрузка входят в первый запрос; следующие передают query и scores.
Это полный cosine scan, ANN-индекса пока нет; ускорение не гарантируется. Парсер, hashing embedder, FTS postings,
WAL и storage пока выполняются на CPU.

### Пользовательские compute-шейдеры

`lin::gpu::GpuCompute` также предоставляет общий native compute API:
`compile_wgsl(source, entry)`, `storage_buffer(bytes)`,
`dispatch(kernel, bind_groups, [x, y, z])`, `read_buffer(buffer)`.
Каждый dispatch оставляет ресурсы на GPU: несколько шейдеров можно выполнять
последовательно, считывая только окончательный результат.
`ComputeKernel::bind_group_layout` возвращает отражённый layout.
`gpu::native` экспортирует используемую версию wgpu; `device()` / `queue()`
дают доступ к uniform/storage buffers, textures, samplers, явным layouts,
command encoders и другим native ресурсам. `checked` преобразует ошибки
валидации и выделения памяти в `lin::Error`; операции внутри него нельзя
вкладывать в другой `checked`. При прямом использовании native API управление
валидацией и синхронизацией остаётся у вызывающего кода.

Проверка пользовательских шейдеров и ошибок на реальном GPU:

```bash
cargo test --release --features gpu --test gpu -- --ignored --nocapture
```

Executor использует compute для числовых/текстовых/regex фильтров, boolean
масок, count, sum, joins, stable sort, lexical/vector/hybrid scoring и ranking,
hop/graph/match. Подготовка строк, Unicode metadata, dictionaries, проекции,
skip/take/union и сборка результата выполняются на CPU. Query pipelines ещё
не объединены в один полностью резидентный GPU-план.

`GpuCompute::with_options(GpuOptions)` задаёт backend, power preference,
optional features и limits; неподдерживаемые требования возвращают ошибку.
Для повторных запросов `upload_vectors(dim, vectors)` создаёт `GpuVectors`,
а `cosine_resident(query, &mut corpus)` передаёт только запрос и считывает
scores. Корпус привязан к устройству; `&mut` предотвращает одновременные записи
в его result buffers. Executor автоматически использует этот кеш;
`Db::gpu_cache_stats()` / `ReadDb::gpu_cache_stats()` показывают uploads/hits.
Каждый снимок получает отдельный кеш. `Db::enable_gpu` подключает устройство
к уже открытому Db; `Db::gpu()` / `ReadDb::gpu()` возвращают backend для
пользовательских compute pipelines.

CLI:

```bash
cargo run --release --features gpu -- run --gpu 'docs | search vec "wal" | take 5'
```

Без feature `gpu` флаг возвращает явную ошибку. Для durable Db используйте
обычный `--data <dir>` перед `run`; GPU-кеш не сохраняется в WAL или snapshot.


Числовые `==`, `!=`, `>`, `<`, `>=`, `<=` и комбинации `and` / `or`
исполняются compute-шейдерами при подключённом GPU, включая `run_batch`,
ReadDb и запросы с `count`. Сравнения f64 выполняются через упорядоченные
64-битные ключи (две компоненты u32), без округления до f32; i64 equality
сохраняет точность выше 2^53. NaN, infinity, signed zero и отсутствующие
значения сохраняют CPU-семантику. Извлечение значений, упаковка ключей,
проекция остаются на CPU; count вычисляется отдельным шейдером. Для числового GPU-фильтра
executor использует общий scan, обходя CPU-оптимизации index/count/project;
ускорение относительно индекса не обещается. Смешанные числовые/текстовые предикаты также используют GPU. `GpuCompute::comparison_dispatches()`
показывает фактическое число отправленных числовых dispatch.


Текстовые equality/inequality, подстрока `~`, `has` и `has` с игнорированием
регистра теперь выполняются шейдером. UTF-8 байты, Unicode-границы слов и
посимвольный lowercase подготавливаются на CPU по тем же правилам Rust,
что исходный executor. Пустые строки и Null сохраняют прежнюю семантику.
Regex также вычисляется GPU NFA-интерпретатором; в смешанном предикате
маски объединяются шейдером boolean. Текст разбивается на пакеты по лимитам
устройства; строка или needle сверх лимита возвращают Error.

`count` и `count by` считают на GPU через atomic counters. Групповые ключи
и их ID готовятся на CPU; каждый пакет возвращает u32 counters, накопленные
в i64 без потери точности. Порядок и представление групп сохраняют CPU API.
`text_dispatches()` / `count_dispatches()` показывают реальные dispatch.

`sum field by group` складывает на GPU в исходном порядке строк каждой группы.
WGSL реализует IEEE-754 binary64 через пары u32, поэтому native f64 не требуется,
включая Metal. Сохраняются округление ties-to-even, signed zero, subnormal,
overflow и infinity; NaN возвращается как canonical quiet NaN (payload не сохраняется).
Конечные результаты проверены побитово против CPU, включая границы пакетов.
Dictionary групп и преобразование Cell в f64 выполняются на CPU по прежним правилам.
Накопитель остаётся на GPU между пакетами; `sum_dispatches()` считает dispatch.
Один invocation последовательно складывает свою группу и просматривает пакет;
это обеспечивает порядок CPU, но не гарантирует ускорение. Размер пакета до 4096 строк;
число групп ограничено буферами и dispatch-лимитами устройства.
Lazy cursors для collection с цепочкой `filter` / `join` / `project` / `skip` / `take`
выполняют поддержанные фильтры и joins на GPU. ID-join читает до 1024 исходных
строк за порцию. При неуникальном FK берётся одна исходная строка за порцию;
она может дать много правых совпадений, которые хранятся до выдачи результатов.
Несколько неуникальных joins могут дополнительно увеличить эту порцию результата.
Открытие курсора не запускает compute; порядок операций сохраняется,
`take` останавливает чтение, ошибки последующих порций возвращаются при чтении.
`next_projected()` сохраняет общую схему полей. Проекции собираются на CPU.
Lazy lex/hybrid search использует GPU matching/scoring; `run`, `run_batch`
и buffered queries используют перечисленные GPU-операции.


`join` / `join left` теперь используют GPU для сопоставления FK-ключей в
`run`, `run_batch`, ReadDb и ленивых курсорах. Ключи кодируются в dictionary IDs на CPU;
отсутствующий ключ получает ID 0 и не совпадает ни с одной строкой.
Для FK на `id` GPU строит lookup через atomicMax, затем выполняет probe;
последний дубликат правой строки выигрывает, как в CPU hash path.
Для неуникальных ключей возвращаются все правые строки в исходном порядке.
Этот путь сопоставляет ключи блоками (O(left × right)); большие ID dictionaries
также используют его, если плотная таблица не помещается в лимит буфера.
Поэтому GPU не гарантирует ускорение относительно CPU hash join.
Сборка итоговых Row и проекция пока выполняются на CPU.
`join_dispatches()` показывает реально отправленные build/probe/matching.
Схема `run_batch` после последней проекции сохраняет порядок колонок
и при пустом результате, включая общий fallback path.


`sort` теперь использует стабильный GPU merge sort: каждый проход выполняет
параллельный binary-search merge над GPU-индексами, без промежуточных readback.
Null, UTF-8 text, bool, vector compact labels, числа и time используют тот же
порядок, что CPU. Числа сортируются по 64-битным ключам, без округления в f32;
signed zero и равные ключи сохраняют исходный порядок в обоих направлениях.
NaN теперь определённо идёт после остальных чисел при ascending (перед ними
при descending); все NaN равны для сортировки. CPU comparator использует ту
же политику, устраняя прежний нетранзитивный порядок NaN.
CPU подготавливает ключи и переставляет Row по окончательным GPU-индексам.
Ключи/текст/индексы должны помещаться в лимиты буферов и dispatch; превышение
возвращает Error. Ускорение относительно CPU sort пока не измерено.
`sort_dispatches()` показывает число реально отправленных merge passes.

`hop`, `graph`, lazy hop и `match` используют резидентные compute-обходы,
описанные ниже. CPU кодирует ключи и собирает Row; `graph_dispatches()` показывает
отправки BFS/DFS. Reverse relations и обратные match hops сохраняют прежнюю семантику.
Lazy hop depth=1 выполняет фильтрацию исходных строк и обход при open;
resolve индексов и выдача строк происходят на CPU. Покрывающий индекс может
устранить исходный фильтрационный dispatch. Ускорение обходов не измерено.

Лексический поиск передаёт проверки подстрок и целых слов в compute-шейдеры,
в том числе для FTS-кандидатов и lazy lex/hybrid cursors. CPU готовит строку
`title + body + snippet`, применяет Rust lowercase и разбивает query по whitespace.
Веса остаются прежними: +1 за substring токена, +2 за целое слово, +3 за всю phrase.
Повторные токены считаются повторно; punctuation-токен не получает бонус целого слова.
Накопление целочисленных весов выполняется GPU-шейдером в резидентном буфере
из двух u32 на score; перенос между младшим и старшим словом сохраняет i64 точность.
`weight_dispatches()` показывает эти отправки. Лексическое ранжирование и top-k используют стабильную GPU-сортировку scores;
RRF также использует GPU reciprocal, binary64 aggregation и sorting;
векторная часть hybrid использует существующий GPU cosine. `text_dispatches()`
подтверждает реальное сопоставление шейдером. Ошибки лимитов/readback возвращаются
через Result, в lazy search — при построении ранжированных индексов на open.
Ускорение относительно CPU lexical scoring не измерено.

Лексический score-аккумулятор сохраняется на GPU между токенами и phrase.
Маски текстового matching передаются аккумулятору прямо в GPU-буферах без
промежуточного readback. Текстовые порции и score-порции могут иметь разные границы:
compute добавляет веса только в соответствующее пересечение диапазонов.
Буферы накопителя разбиваются по storage/dispatch-лимитам; окончательные scores
читаются после всех весов. Переполнение диапазона i64 возвращает Error.

Лексический поиск ранжирует scores стабильным GPU merge sort по точным i64-ключам,
без преобразования в f64; равные scores сохраняют порядок исходных строк.
Для FTS сохраняется порядок `score desc, row index asc`. Top-k выбирается из
GPU-отсортированного списка, вместо CPU heap. Это полная сортировка кандидатов,
а не отдельный GPU partial-selection алгоритм; действуют лимиты буферов сортировки.
`sort_dispatches()` подтверждает ранжирование, в том числе при открытии lazy search.
Фильтрация нулевых scores, выбор префикса индексов и сборка Row остаются на CPU.

Hybrid reciprocal rank fusion теперь вычисляется compute-шейдером: denominator
`61 + rank` передаётся как u32, GPU выполняет корректно округлённое `1 / denominator`
и последовательно складывает binary64-веса каждой identity. Native f64 не требуется;
используется длинное целочисленное деление и та же software binary64 addition,
что в `sum`. Проверены побитовые результаты против CPU, включая большие denominators,
повторные identities и границы пакетов. `rrf_dispatches()` показывает отправки.
Итоговый список сортируется GPU с прежними tie-breaks: source index для FTS,
identity string для residual rows. Identity dictionary и сборка Row выполняются на CPU.
Rank должен помещаться в u32 вместе с 61; лимиты групп/сортировки возвращают Error.
Ускорение не измерено; накопление группы последовательно просматривает пакет.

Regex predicates используют Thompson NFA из regex-automata: переходы по байтам,
alternation, repetitions, epsilon closure и поиск совпадения выполняются WGSL
compute-интерпретатором. Captures не нужны для boolean `is_match`; greediness
не меняет существование совпадения. Unicode-классы компилируются в байтовый NFA,
а look assertions (anchors, CRLF, ASCII/Unicode word boundaries) готовятся на CPU
тем же LookMatcher, что Rust regex. Empty matches разрешены только на UTF-8 границах;
Null всегда false. RegexBuilder проверяет прежнюю семантику flags/invalid patterns:
внешний `i` включает ignore-case, некорректный regex возвращает false.
`regex_dispatches()` показывает реальные NFA dispatch. Pure/mixed regex, `run_batch`,
ReadDb и GPU lazy cursors используют этот путь. Program, scratch и text ограничены
лимитами GPU; превышение возвращает Error без CPU matching fallback.
Каждый invocation последовательно симулирует NFA одной строки; scratch выделяется
порциями по лимитам. Скорость относительно Rust regex не измерена.

`hop` и `graph` в обычных/batch/ReadDb запросах теперь управляют BFS в compute:
frontier, seen nodes, reached nodes и seen edge identities хранятся на GPU.
Один invocation последовательно сканирует рёбра в исходном порядке на каждом уровне,
с прежними правилами cycles, dedup и предела 300 результатов. Для hop ограничение
проверяется после уровня, для graph — после каждого emitted edge, как на CPU.
CPU готовит dictionaries и resolves достигнутые ключи в Row. Рёбра и состояние
полного walk должны помещаться в storage buffers; превышение возвращает Error.
Это резидентный последовательный GPU walk, не параллельный BFS; ускорение не измерено.
Lazy hop depth=1 разделяет резидентный walk обычного hop: CPU не обновляет
frontier/seen после отправки. Итоговые ключи превращаются в индексы строк;
курсор остаётся lazy для выдачи Row/ProjectedRow и применяет skip/take при чтении.

`match` теперь выполняет DFS каждого start key на GPU: стек кадров и ancestor path
хранятся в storage buffer, cycles исключаются шейдером, LIFO-порядок сканирования
и повторные пути сохраняются. Глубина 1..3 соответствует compiler check.
CPU готовит presence table разрешимых узлов, позволяя GPU пропускать emission
неразрешимых узлов и продолжать их обход. Каждая expansion получает остаток
лимита 300; итоговые node/edge indices разрешаются в aliases и Row на CPU.
Последовательность hops и input rows оркестрирует CPU, без CPU DFS/path sets.
Stack/program/output сверх лимитов возвращают Error; native f64/features не нужны.
Это последовательный GPU DFS, ускорение относительно CPU не измерено.

Если `hop` разрешает соседей одновременно в `docs` и исходной коллекции,
курсор использует buffered result: его lazy index source адресует только одну
коллекцию. Сам обход остаётся compute-операцией; строки и их порядок совпадают
с обычным `run`, включая ReadDb.

Векторный поиск сортирует оценки cosine через тот же stable compute merge sort,
что и `sort`; равные оценки сохраняют порядок исходных строк. Этот путь общий
для `search vec` и векторной части hybrid, включая ReadDb. CPU готовит ключи
и собирает строки по возвращённым GPU индексам. Hardware-проверка отдельно
проверяет рост `sort_dispatches()` при выполнении поиска.

Для reflected layouts с обработкой неверного индекса используйте
`gpu.bind_group_layout(&kernel, index) -> Result<BindGroupLayout, Error>`.
Метод `kernel.bind_group_layout(index)` сохраняет native wgpu API и должен
вызываться с корректным индексом или внутри `gpu.checked(...)`.

`cargo test --features gpu --test shaders` проверяет все WGSL-файлы в
`src/shaders` без адаптера: синтаксис, Naga validation и baseline capabilities.
Workflow `.github/workflows/compute.yml` добавляет эту проверку, сборку native
API/примеров и сборку без GPU для Linux/macOS/Windows. Hardware-тесты запускаются
отдельно командой `cargo test --release --features gpu --test gpu -- --ignored`;
обычный CI не является доказательством исполнения на физическом GPU.
