# Lin

Lin — язык своей локальной БД: пайпы, типизированный каталог, два мира записи (`append` / reducer), свой план. Не SQL и не Kusto.

**0.2** — встраиваемый локальный прототип с durable `--data`, backup export/import и честным lex-search. Не multi-writer / не сеть.

Сейчас есть парсер, typecheck, IR плана, in-memory store (один reducer, один `gen`), исполнитель, WAL+snapshot, `backup` CLI. DuckDB/IDB/WASM backends в этом milestone нет. Векторный search в плане не обещается: `search` → lex (`hybrid→lex (no embedder)`).

## Стабильный API (0.2)

Публичный контракт: `Db::{empty,fixture,open,close,run,prepare,explain_as,export_backup,import_backup,import_backup_into,stats}`, плюс `parse` / `compile` / `run` / `explain*`, типы `Handle` / `Done` / `Prepared` / `Error` / `Row` / `Cell` / `Store` / `Plan` / `Stats` / `VERSION`.

## Лаконичный диалект

После `|` голый предикат — это `where`. В запросе `{ id, title }` — проекция. `where` и `pick` остаются синонимами.

```
docs | wing == "rag" and ts > ago 7d | { id, title, room }
docs | id == "e7c98d54-b4d6-4165-86e9-9b999e7ce9c3"
docs | title has "wal"
docs | title ~ "wal"
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

Неявный `take 50`. `search` / `search hybrid` исполняются **только lex** (эмбеддера нет); в explain: `Search lex` + `hybrid→lex (no embedder)`. `search vec` планируется, но возвращает пусто. `embed_id=nomic-embed-text/768` — метка каталога, не живой embedder.

## Roadmap (кратко)

| Фаза | Статус |
|---|---|
| P0 semver 0.2 + честный search | **сейчас** |
| P1 backup export/import + stats | **сейчас** |
| P1 multi-reader / compaction | дальше |
| P2 mmap/cold + quotas | позже |
| P3 сеть / MVCC | другой продукт |

## Бенчмарки (rbench): Lin vs SQLite vs DuckDB vs Postgres vs MySQL

Сравнительные hot paths в `benches/compare.rs` через [rbench](https://github.com/themoretheless/rbench) (`Suite`, `harness = false`).

```bash
# список кейсов
cargo bench --bench compare -- --list

# быстрый прогон (нужен --release; rbench отказывается от debug)
cargo bench --bench compare -- --profile quick

# только point get / только insert / только append_log
cargo bench --bench compare -- --filter point_get
cargo bench --bench compare -- --filter insert_bulk_1k --samples 8
cargo bench --bench compare -- --profile quick --filter append_log
```

Движки: **Lin**, **SQLite** (`rusqlite` bundled), **DuckDB** (bundled; собирается на mac aarch64), **Postgres** / **MySQL** (опционально, через URL), плюс **HashMap** только для point get. N=10 000 для тёплых чтений (fixture; setup вне тайминга). Bulk insert: схема/индекс в setup, в тайминге только запись.

Сравнимо: point get по id, `wing ==`, range `wing`+`ts`, substring (`title ~ "wal"` ≈ `LIKE '%wal%'`), materialize `SELECT id,title`, bulk insert 1k/10k, **append_log** (`append facts` vs `INSERT INTO logs`) 1k/10k.  
Не сравниваем здесь (и не подтасовываем): Lin `hop`/`match`, real vec/hybrid, CAS, durable fsync (`--data`) — отдельный слой.

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

Зависимости бенча: `postgres`, `mysql` (dev-dependencies). `rbench` — path на sibling `../rbench/crates/rbench`.

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

CLI: `lin run '<запрос>'` печатает строки и `gen`. `lin run --file batch.lin` и `lin run -` читают программу из файла / stdin. `lin explain '<запрос>'` печатает план или ошибку компиляции. `lin --data .lin run '…'` пишет в каталог данных. `lin backup export|import`, `lin stats`, `lin version`.

## Backup

```bash
lin --data .lin backup export /tmp/lin-bak.json
lin backup import /tmp/lin-bak.json --data .lin2
lin --data .lin2 stats
```

Формат — JSON snapshot (collections + edges + gen). Export перед записью делает checkpoint, если store durable.

## Данные на диске

Без `--data` store эфемерный (fixture в памяти) — так живут текущие тесты языка и исполнителя.

`--data <dir>` (привычный путь `./.lin`) открывает durable store: `Store::open`, replay в память, дальше тот же исполнитель. Запись сначала `fsync` записи лога, потом инкремент `gen`. Крах до fsync = записи не было.

```
.lin/
  head       JSON: gen, catalog_hash, embed_id
  log        append-only: u32 LE длина + JSON пакета
  snapshot   опциональный чекпоинт всего store (после 32 записей или close)
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
