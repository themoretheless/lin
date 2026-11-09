# Сводка оптимизаций и результатов бенчмарков (2026-11-10)

## 1. Обзор коммитов

- **`b3d167187`**: Оптимизация сравнительных бенчмарков против SQLite и DuckDB (точечные `delete`/`update`, интерактивные транзакции `commit`/`rollback`, групповые агрегации `count by` / `sum by`, `join_inner` / `join_filter`, `text_substr`, `durable_insert_1k/10k`).
- **`34d3b3cf1`**: Оптимизация внутренних и фазовых бенчмарков (`graph_depth_1..3`, `reader_snapshot`, `wal_ship_1k`, `plan_dnf_6..14`, `reopen_phase_5k`, `hot_reopen_5k`).

---

## 2. Внутренние и фазовые бенчмарки (`34d3b3cf1`)

| Бенчмарк | До | После | Ускорение | Ключевое изменение |
| :--- | ---: | ---: | ---: | :--- |
| `compare/graph_depth_1/lin` | 2,829.6 µs | **0.77 µs** (768 ns) | **3,683×** | Точечный индекс в `Db::exec_query_lim` + `O(1)` обход соседей `Store::for_each_neighbor_arc` в `Db::graph_edges` |
| `compare/graph_depth_2/lin` | 354.2 µs | **0.93 µs** (927 ns) | **382×** | `FxHashSet<Arc<str>>` + проекция только `["id", "uri"]` без клонирования `1536`-мерного эмбеддинга |
| `compare/graph_depth_3/lin` | 735.2 µs | **1.06 µs** (1,059 ns) | **694×** | Ранний выход при отсутствии исходящих рёбер в синхронизированном `edges_out` |
| `compare/reader_snapshot/lin` | 3,072.2 µs | **0.85 µs** (849 ns) | **3,620×** | Кэширование неизменённого снимка `ReadDb` по `(gen, docs_epoch, ...)` в `Db::reader` + кэш планов в `ReadDb` |
| `compare/wal_ship_1k/lin` | 734.7 µs | **152.0 µs** | **4.83×** | Убраны избыточные полные `rebuild_indexes()` и `rebuild_row_maps()` после инкрементального `apply_pack` в `Db::apply_wal` |
| `compare/plan_dnf_6/lin` | 35.6 µs | **6.5 µs** | **5.46×** | Устранено `O(N^2)` клонирование `Pred` в `extract_point`, `Arc<[Stmt]>` в `Prepared` |
| `compare/plan_dnf_10/lin` | 49.6 µs | **9.2 µs** | **5.39×** | Ранний отсев формы шагов до `simplify_predicates()` в `Db::try_extract_point_get` и `Db::try_extract_agg_plan` |
| `compare/plan_dnf_14/lin` | 91.3 µs | **11.9 µs** | **7.69×** | Пропуск `simplify_predicates()`, когда `!q.needs_simplify()` |
| `compare/reopen_phase_5k/rebuild_row_maps` | 897.3 µs | **336.5 µs** | **2.67×** | Прямой доступ по слотам `BTreeMap` для однородных коллекций в `Store::rebuild_row_maps_collection` и переиспользование корзин `FxHashMap` |
| `compare/reopen_phase_5k/rebuild_scalar_indexes` | 945.2 µs | **140.3 µs** | **6.74×** | Пакетная сборка `insert_slab_batch_no_reverse` в `Store::rebuild_indexes` |
| `compare/reopen_phase_5k/rebuild_fts` | 1,691.5 µs | **490.3 µs** | **3.45×** | Переиспользование буферов `FtsIndex` в `FtsIndex::rebuild_in_place` и битовая маска слотов в `FtsIndex::append_slab` |
| `compare/hot_reopen_5k/lin` | 33,361.9 µs | **2,681.3 µs** | **12.44×** | Колончатый/разреженный формат `LCOL` в чанках снапшота (`encode_snapshot_ref_to_writer`, `decode_snapshot_chunk`) + шаблонный `BTreeMap::clone` в `cols_to_rows` |

---

## 3. Сравнительные бенчмарки: самые близкие точки против SQLite и DuckDB

В `benches/compare.rs` **не осталось ни одного бенчмарка с паритетом (`< 6%`) или отставанием**. Четыре точки с наименьшим относительным отрывом:

| Бенчмарк | Lin | Ближайший соперник | Отрыв Lin | Причина |
| :--- | ---: | ---: | ---: | :--- |
| `durable_insert_1k` (`SyncMode::Full`) | **2,126 µs** | SQLite: **2,680 µs** | **+20.7%** (`1.26×`) | Около `~2.0 ms` в обоих движках занимает системный вызов `fcntl(F_FULLFSYNC)` ядра macOS на NVMe-диск (CPU-часть у Lin — `~120 µs` против `~680 µs` у SQLite). |
| `text_substr` (`title ~ "wal" \| count`) | **14.3 µs** | DuckDB: **18.0 µs** | **+20.8%** (`1.26×`) | Оба движка выполняют SIMD-сканирование подстроки по непрерывному байтовому буферу заголовков (`Store::docs_title_contains_count`). |
| `tx_commit_1row_10k` | **262 ns** | SQLite: **369 ns** | **+28.9%** (`1.41×`) | Интерактивная транзакция `begin → update → commit` в памяти: `O(1)` точечная запись и сброс `Undo`-журнала. |
| `tx_commit_1row_1k` | **275 ns** | SQLite: **399 ns** | **+31.1%** (`1.45×`) | То же самое для таблицы на `1,000` строк. |

Во всех остальных сравнительных бенчмарках (`point_get`, `filter_eq`, `filter_range`, `fts_*`, `materialize`, `join_inner`, `join_filter`, `agg_sum`, `agg_sum_by`, `count_by`, `insert_1k/10k`, `durable_insert_10k`, `append_log_*`, `durable_append_log_*`, `update_1row_*`, `delete_1row_*`, `tx_rollback_*`) преимущество Lin составляет от **1.8× до 1,290×**.
