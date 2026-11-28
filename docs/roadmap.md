# Lin — план развития

Срез: **2026-11-28**, ветка `main`. Единственный файл с планом в репозитории: в нём хранится только несделанное. Всё реализованное перенесено в [CHANGELOG](../CHANGELOG.md), профильные документы `docs/` ([api-and-feature-matrix.md](api-and-feature-matrix.md), [storage-and-compatibility.md](storage-and-compatibility.md), [query-semantics.md](query-semantics.md), [migration-and-operations-runbooks.md](migration-and-operations-runbooks.md), [benchmark-and-evidence-guide.md](benchmark-and-evidence-guide.md)) и отчёты `benches/results/` ([2026-11-20-roadmap-acceptance](../benches/results/2026-11-20-roadmap-acceptance/report.md)).

Открыто **378** пунктов общего backlog (часть II: в основном сравнительные `G-PERF` замеры из ≥6 чередующихся process-pairs и платформенно/внешне заблокированные проверки) и **1** платформенно-зависимый замер части I (`P4.2` на физическом Linux `ext4` / `xfs`). Это план работ и гипотез, не список подтверждённых дефектов и не обещание ускорений без приёмочного замера.

ID `LIN-xxxx` и `Rank` стабильны и не перенумеровываются (закрытые пункты удалены). Перед взятием пункта со статусом `candidate` или `qualification` сначала сверять с текущим кодом.

## Часть I. Производительность, хранение и OLAP-контур

### 1. Текущие архитектурные ограничения OLAP

- Полный eager in-memory SoA на этапе записи специализирован под встроенные коллекции (`docs`, `orders`, `users`, `facts`); произвольные пользовательские коллекции со схемой `Catalog` используют построчный `Vec<Row>` как источник истины и автоматически строят ленивое типизированное колоночное зеркало `ColMirror` (`DenseInt`, `DenseFloat`, `DenseTime`, `DenseBool`, `DenseText`, `Dict`) при чтении от `256` строк (либо явно через `Db::declare_columns` / `Db::enable_catalog_columns`).
- Многошаговые конвейеры `Filter` / `Project` / `Without` / `Sort` / `Skip` / `Take` (включая post-join цепочки) исполняются векторизованно над типизированными колонками `RecordBatch`; операторы за пределами этого набора (произвольные арифметические `extend`, сложные вложенные предикаты после `join` или многосторонние non-FK соединения) откатываются на `Vec<Row>`.

### 2. Открытые платформенные замеры части I

#### P4.2. Замер WAL фиксированной длины на Linux (`ext4` / `xfs`)
- Предвыделение экстентов `fallocate(FALLOC_FL_KEEP_SIZE)` внедрено в `reserve_wal_space`; остаётся провести нативный сравнительный замер на физическом Linux-хосте (`ext4` / `xfs`, `FALLOC_FL_KEEP_SIZE` + `fdatasync` против фиксированной длины с кадром-терминатором) и менять формат кадра-терминатора только при подтверждённом выигрыше.

| Пункт | Сложность | Ожидаемый эффект | Целевые проверки и бенчмарки |
|---|---|---|---|
| **P4.2** Замер preallocated WAL на Linux (`ext4` / `xfs`) | Средняя | Оценка `FALLOC_FL_KEEP_SIZE` vs кадр-терминатор на Linux | `durable_insert_*` на Linux |

### 3. Постоянные ворота приёмки

- `cargo check --all-targets` и `--all-features` (`0` предупреждений), `cargo test --release --all-features` (`100%` pass), `./scripts/format-compat.sh` (`100%` pass) и `AIRBUG_HUB="" ./scripts/ci-bench.sh` (`exit 0`).

## Часть II. Общий backlog

### Исполнимая очередь

| Пакет | Задачи | Конкретный результат и условие перехода |
|---|---|---|
| 4. Сохранность данных | D02–D06, D43 | Переиспользовать существующие oracle/fault injection; дополнить конкретные пробелы. Последний подтверждённый commit, mixed codecs, checkpoint, backup/follower и ошибки диска. Воспроизведённая потеря данных опережает performance. |
| 5. Память записи | LIN-0321, LIN-0322, LIN-0349 | Сначала измерить allocations, hit rate и retained bytes существующих пулов. Любой новый reuse должен пройти cold start, полный жизненный цикл с Drop, лимиты, alias isolation и ошибки. Не переносить освобождение памяти за таймер ради результата. |
| 6. Группировки | D19 | Расширять покрытие от уже принятого fixed-text single-key COUNT: много ключей, другие агрегаты, mixed numeric/NaN/zero, Unicode и quota exhaustion. Не считать узкий index-only fast path реализацией произвольного GROUP BY. |
| 7. Все восемь peers | LIN-0006, D01 | SQLite, DuckDB, PostgreSQL, MySQL, MSSQL, MongoDB, Kusto, pandas: одинаковый объявленный контракт и независимый readback. MSSQL — реальный native CI, comparative evidence приватно. Kusto — только реальный разрешённый endpoint; до него измерение отсутствует. |
| 8. Запросы и поиск | D10–D28, D31 | Выбирать по профилю: selectivity/skew, cardinality, ширина строк, index maintenance, joins/windows, FTS/vector и graph. Новая функция требует пользовательского сценария; ускорение — контроля fallback и стоимости обновлений. |
| 9. Ограниченные ресурсы | D09, D17–D18, D29–D36, D45 | RSS, streaming/backpressure, cancellation, readers/writer, CPU/GPU equivalence, replication recovery. Поддержка компиляции GPU не равна проверке на физическом устройстве. |
| 10. Поставка и API | D37–D44, D46–D50 | Стабильные API, schema/import/export, CLI и диагностика, воспроизводимая сборка, platform CI и документация. После каждого среза — runnable пример, релевантные проверки и ссылка на evidence. |

### Правило выбора следующего пункта

Сначала подтверждённая потеря данных или нарушение контракта, затем измеренный основной bottleneck, затем востребованная функция. При равном риске выбирать меньший самостоятельный результат. Области по 40 пунктов — структура каталога, а не требование завершить область целиком перед следующей.

Для каждого взятого пункта записывать: воспроизводитель/потребность → точный scope → зависимости → изменение → oracle и gate → raw evidence → итоговый статус и ограничения. Уже существующий механизм получает `covered` после проверки, отвергнутый эксперимент не повторяется без изменения гипотезы. Новые дефекты не объявляются найденными только потому, что их проверка включена в backlog.

Производительность принимать по [контракту бенчмарков](benchmark-contract.md). Новый [платформенный CI gate](../benches/results/2026-10-26-linux-ci-refresh/report.md) отделяет исторические абсолютные лимиты от same-run сравнений SQLite/DuckDB; изменение gate само по себе не ускоряет БД. Превосходство над всеми восемью peers остаётся отдельным незавершённым итоговым условием.

### Как исполнять

1. Начать с первых задач ниже; одновременно вести не более одной performance-гипотезы в одном benchmark checkout.
2. Перед изменением записать контракт, источники, baseline, ресурсы и критерий отказа. Пункт `qualification` не предполагает заранее наличия ошибки.
3. Сохранять API/ошибки/порядок/битовую точность там, где они являются контрактом; отдельное изменение семантики требует явного дизайна и миграции.
4. Не ослаблять sync, не отключать embedding/FTS/индексы и не исключать проигрыши ради победы. Новые peer-контракты описывать отдельно.
5. После узкой проверки запускать релевантные интеграционные проверки; ускорение сохранять только после независимого повтора с контролем. Отрицательный результат сохранять как evidence.
6. Блокированные endpoints не считать победами; пароли/токены в roadmap не помещать.
7. После каждого принятого изменения обновлять затронутые измерения и статус задач, не сдвигая стабильные ID.
8. Завершение всего backlog не равно автоматически завершению benchmark-цели: нужен отдельный audit всех named peers и сценариев.

### Волны

- **Волна A:** current baseline, CRC/format и Cell Hash/Eq qualification, profiling WAL, unmatched peer contracts.
- **Волна B:** native 1k и durable 1k/10k; сначала текущий профиль, затем по одному кандидату.
- **Волна C:** повторить read matrix и write coverage, подключить MSSQL/Kusto после настройки доступа.
- **Волна D:** ресурсы и корректность indexes/FTS/ANN/cursors/async, differential/crash harness.
- **Волна E:** cost model, spill, schema evolution, расширения API и эксплуатации.
- **Волна F:** внешние bindings, упаковка и широкая platform qualification.

Пункты из разных волн могут понадобиться раньше при подтверждённом дефекте. Здесь нет календарного обещания реализовать все 2000.

### Обозначения

- `P1` — первый triage: текущий bottleneck, совместимость, ресурсы или потенциально значимый инвариант. Не синоним подтверждённого critical bug.
- `P2` — следующий слой развития и квалификации.
- `P3` — дальний backlog, только после проверки полезности и crossover.
- `measured_gap` — конкретный проигрыш из текущего отчёта; `qualification` — ещё не проверенная граница; `candidate` — идея/дизайн; `blocked_external` — требуется внешний доступ.
- `S/M/L` — приблизительный объём исследования, не срок и не измеренная трудоёмкость.
- Связь с файлом означает релевантный модуль для triage, не найденный дефект на каждой его строке.

### Критерии готовности

- **G-PERF** (Оптимизации и измерительные гипотезы): Независимый oracle сохраняет значения/порядок/ошибки; baseline/candidate одного контракта, два набора ≥6 чередующихся process-pairs, контроль SQLite, 1k/10k native+durable при изменении записи; сохранить raw samples/hash/версии/RSS. Смешанный результат не считать ускорением.
- **G-CORRECT** (Корректность и границы): Сначала минимальный reproducer или формальная ambiguity, затем согласованный контракт и независимый oracle. До исправления тест воспроизводит проблему; после исправления проходят релевантные rollback/reopen/replay и границы. Если реализация уже корректна, записать covered, не менять код.
- **G-FEATURE** (Новые функции и дизайн): Сначала проверить наличие в текущем коде. Если отсутствует, описать API/семантику/ресурсные лимиты/ошибки/совместимость; затем сценарий пользователя и end-to-end проверка. Существующую функцию не реализовывать повторно.
- **G-API** (API и типизированные интерфейсы): Публичный пример и compile-time/runtime контракт; проверить downstream использование, стабильные типы, serialization и feature-disabled режим; зафиксировать совместимость и ошибки.
- **G-VERIFY** (Qualification и независимые проверки): Самостоятельный oracle/инвариант, детерминированный seed и минимизируемый fixture; намеренное нарушение ловится проверкой, реальная операция проходит. Сохранить failure artifact; green-only запуск без чувствительности к ошибке недостаточен.
- **G-OPS** (Эксплуатация и инструменты): Воспроизводимый сценарий запуска/отказа/очистки, сохранённые diagnostics и resource bounds. Не менять чужие объекты; секреты не попадают в вывод. Проверить нормальное завершение и отмену.
- **G-DOC** (Документация и доказуемые утверждения): Связать утверждения с текущим API, runnable примером или raw evidence; проверить ссылки и scope. Не публиковать старый результат как текущий или component microbench как скорость всей БД.

Для каждой строки итоговый критерий — её конкретный результат плюс указанный gate. Если критерий требует недоступной платформы/endpoint, статус остаётся непроверенным; mock/compile не заменяет live qualification.

### Приоритетный список — 378 пунктов

Количества: P1: 1, P2: 257, P3: 120; статусы: blocked_external: 1, candidate: 339, qualification: 38.

#### P1: 1 пунктов

| Rank | ID | Область | Задача | Тип | Статус | Объём | Gate |
|---:|---|---|---|---|---|---|---|
| 22 | LIN-0006 | [D01](#d01) | Получить тестовый endpoint Kusto | OPS | blocked_external | M | G-OPS |

#### P2: 257 пунктов

| Rank | ID | Область | Задача | Тип | Статус | Объём | Gate |
|---:|---|---|---|---|---|---|---|
| 111 | LIN-0009 | [D01](#d01) | Разделить CPU и RPC latency | PERF | candidate | M | G-PERF |
| 112 | LIN-0010 | [D01](#d01) | Разделить prepare и execute latency | PERF | candidate | M | G-PERF |
| 113 | LIN-0011 | [D01](#d01) | Уравнять подготовку DuckDB INSERT | PERF | candidate | M | G-PERF |
| 114 | LIN-0012 | [D01](#d01) | Уравнять создание входных DataFrame | PERF | candidate | M | G-PERF |
| 116 | LIN-0014 | [D01](#d01) | Сохранять runtime feature flags | PERF | candidate | M | G-PERF |
| 117 | LIN-0015 | [D01](#d01) | Проверять sample operation counts | PERF | candidate | M | G-PERF |
| 118 | LIN-0016 | [D01](#d01) | Измерять cold constructor latency | PERF | candidate | M | G-PERF |
| 122 | LIN-0020 | [D01](#d01) | Отдельно измерять 1M-row ingestion | PERF | candidate | M | G-PERF |
| 123 | LIN-0021 | [D01](#d01) | Измерять wide-row insertion | PERF | candidate | M | G-PERF |
| 124 | LIN-0022 | [D01](#d01) | Измерять skewed-key insertion | PERF | candidate | M | G-PERF |
| 125 | LIN-0023 | [D01](#d01) | Измерять крупные embedding payloads | PERF | candidate | M | G-PERF |
| 126 | LIN-0024 | [D01](#d01) | Измерять длительный mutation churn | PERF | candidate | M | G-PERF |
| 127 | LIN-0025 | [D01](#d01) | Сопоставить steady-state RSS peers | PERF | candidate | M | G-PERF |
| 128 | LIN-0026 | [D01](#d01) | Сопоставить peak RSS peers | PERF | candidate | M | G-PERF |
| 129 | LIN-0027 | [D01](#d01) | Сопоставить CPU-time peers | PERF | candidate | M | G-PERF |
| 130 | LIN-0028 | [D01](#d01) | Сопоставить cold-start стоимости peers | PERF | candidate | M | G-PERF |
| 131 | LIN-0031 | [D01](#d01) | Исключать фоновую компиляцию автоматически | PERF | candidate | M | G-PERF |
| 132 | LIN-0032 | [D01](#d01) | Сохранять признаки thermal throttling | PERF | candidate | M | G-PERF |
| 133 | LIN-0033 | [D01](#d01) | Рандомизировать порядок сравнимых cases | VERIFY | qualification | S | G-VERIFY |
| 134 | LIN-0034 | [D01](#d01) | Проверять независимость повторных процессов | PERF | candidate | M | G-PERF |
| 135 | LIN-0035 | [D01](#d01) | Показывать полное распределение samples | PERF | candidate | M | G-PERF |
| 136 | LIN-0036 | [D01](#d01) | Отделять batch averages от percentiles | PERF | candidate | M | G-PERF |
| 137 | LIN-0037 | [D01](#d01) | Ввести бюджет неопределённости результата | PERF | candidate | M | G-PERF |
| 138 | LIN-0038 | [D01](#d01) | Проверять incomplete при пропавшем peer | PERF | candidate | M | G-PERF |
| 139 | LIN-0039 | [D01](#d01) | Проверять loss gate при проигрыше | PERF | candidate | M | G-PERF |
| 140 | LIN-0040 | [D01](#d01) | Автоматизировать итоговый completion audit | PERF | candidate | M | G-PERF |
| 157 | LIN-0061 | [D02](#d02) | Уменьшить vector frame copies | CORRECTNESS | qualification | M | G-CORRECT |
| 158 | LIN-0062 | [D02](#d02) | Переиспользовать точный serialized size | CORRECTNESS | qualification | M | G-CORRECT |
| 159 | LIN-0063 | [D02](#d02) | Измерить segmented WAL encoding | PERF | candidate | M | G-PERF |
| 160 | LIN-0064 | [D02](#d02) | Измерить reserve buffer policy | PERF | candidate | M | G-PERF |
| 178 | LIN-0086 | [D03](#d03) | Проверить fdatasync semantics Linux | CORRECTNESS | qualification | S | G-CORRECT |
| 181 | LIN-0089 | [D03](#d03) | Сопоставить acknowledged durable boundaries | PERF | candidate | M | G-PERF |
| 187 | LIN-0095 | [D03](#d03) | Измерить header coalescing | PERF | candidate | M | G-PERF |
| 188 | LIN-0096 | [D03](#d03) | Измерить reusable frame buffers | PERF | candidate | M | G-PERF |
| 189 | LIN-0097 | [D03](#d03) | Измерить WAL preallocation | PERF | candidate | M | G-PERF |
| 190 | LIN-0098 | [D03](#d03) | Измерить append file fragmentation | PERF | candidate | M | G-PERF |
| 191 | LIN-0099 | [D03](#d03) | Измерить buffered против direct I/O | PERF | candidate | M | G-PERF |
| 192 | LIN-0100 | [D03](#d03) | Измерить sequential write advice | PERF | candidate | M | G-PERF |
| 194 | LIN-0102 | [D03](#d03) | Разделить throughput и sync latency | PERF | candidate | M | G-PERF |
| 197 | LIN-0105 | [D03](#d03) | Сопоставить групповой commit fairness | PERF | candidate | M | G-PERF |
| 206 | LIN-0114 | [D03](#d03) | Проверить file descriptor exhaustion | CORRECTNESS | qualification | S | G-CORRECT |
| 209 | LIN-0117 | [D03](#d03) | Измерить tiny WAL commits | PERF | candidate | M | G-PERF |
| 210 | LIN-0118 | [D03](#d03) | Измерить large WAL commits | PERF | candidate | M | G-PERF |
| 211 | LIN-0119 | [D03](#d03) | Измерить concurrent-reader sync impact | PERF | candidate | M | G-PERF |
| 212 | LIN-0120 | [D03](#d03) | Измерить fsync на реальном диске | PERF | candidate | M | G-PERF |
| 233 | LIN-0145 | [D04](#d04) | Измерить bounded replay memory | PERF | candidate | M | G-PERF |
| 234 | LIN-0146 | [D04](#d04) | Измерить hot replay throughput | PERF | candidate | M | G-PERF |
| 235 | LIN-0147 | [D04](#d04) | Измерить cold replay throughput | PERF | candidate | M | G-PERF |
| 236 | LIN-0148 | [D04](#d04) | Профилировать metadata rebuild replay | PERF | candidate | M | G-PERF |
| 257 | LIN-0173 | [D05](#d05) | Измерить incremental checkpoint | PERF | candidate | M | G-PERF |
| 258 | LIN-0174 | [D05](#d05) | Измерить section-local compression | PERF | candidate | M | G-PERF |
| 259 | LIN-0175 | [D05](#d05) | Измерить parallel snapshot sections | PERF | candidate | M | G-PERF |
| 279 | LIN-0215 | [D06](#d06) | Измерить rollback work amplification | PERF | candidate | M | G-PERF |
| 280 | LIN-0216 | [D06](#d06) | Измерить journal clone elimination | PERF | candidate | M | G-PERF |
| 305 | LIN-0245 | [D07](#d07) | Измерить borrowed identity sets сейчас | PERF | candidate | M | G-PERF |
| 306 | LIN-0246 | [D07](#d07) | Измерить defaults materialization batching | PERF | candidate | M | G-PERF |
| 307 | LIN-0247 | [D07](#d07) | Измерить body hash cost | PERF | candidate | M | G-PERF |
| 308 | LIN-0248 | [D07](#d07) | Измерить staged field validation | PERF | candidate | M | G-PERF |
| 320 | LIN-0260 | [D07](#d07) | Проверить large bulk memory release | CORRECTNESS | qualification | S | G-CORRECT |
| 321 | LIN-0261 | [D07](#d07) | Измерить streaming bulk input | PERF | candidate | M | G-PERF |
| 325 | LIN-0265 | [D07](#d07) | Измерить changed-field update masks | PERF | candidate | M | G-PERF |
| 326 | LIN-0266 | [D07](#d07) | Измерить no-op update fast path | PERF | candidate | M | G-PERF |
| 327 | LIN-0267 | [D07](#d07) | Измерить multirow delete compaction | PERF | candidate | M | G-PERF |
| 328 | LIN-0268 | [D07](#d07) | Измерить swap-remove map repair | PERF | candidate | M | G-PERF |
| 335 | LIN-0275 | [D07](#d07) | Проверить mutation returning old/new values | CORRECTNESS | qualification | S | G-CORRECT |
| 336 | LIN-0276 | [D07](#d07) | Проверить mutation counters overflow | CORRECTNESS | qualification | S | G-CORRECT |
| 349 | LIN-0293 | [D08](#d08) | Измерить SSO field length distribution | PERF | candidate | M | G-PERF |
| 350 | LIN-0294 | [D08](#d08) | Измерить Cell size regression | PERF | candidate | M | G-PERF |
| 351 | LIN-0295 | [D08](#d08) | Измерить cloned inline strings | PERF | candidate | M | G-PERF |
| 352 | LIN-0296 | [D08](#d08) | Измерить shared large strings | PERF | candidate | M | G-PERF |
| 360 | LIN-0305 | [D08](#d08) | Спроектировать explicit decimal Cell | FEATURE | candidate | L | G-FEATURE |
| 361 | LIN-0306 | [D08](#d08) | Спроектировать binary payload Cell | FEATURE | candidate | L | G-FEATURE |
| 362 | LIN-0307 | [D08](#d08) | Спроектировать duration Cell | FEATURE | candidate | L | G-FEATURE |
| 363 | LIN-0308 | [D08](#d08) | Спроектировать UUID scalar handling | FEATURE | candidate | L | G-FEATURE |
| 364 | LIN-0309 | [D08](#d08) | Спроектировать field-name interning boundary | FEATURE | candidate | L | G-FEATURE |
| 366 | LIN-0311 | [D08](#d08) | Спроектировать stable typed field IDs | FEATURE | candidate | L | G-FEATURE |
| 367 | LIN-0312 | [D08](#d08) | Спроектировать row version metadata | FEATURE | candidate | L | G-FEATURE |
| 372 | LIN-0317 | [D08](#d08) | Измерить inline-key layout alternatives | PERF | candidate | M | G-PERF |
| 373 | LIN-0318 | [D08](#d08) | Измерить hot/cold row metadata | PERF | candidate | M | G-PERF |
| 374 | LIN-0319 | [D08](#d08) | Измерить compressed null metadata | PERF | candidate | M | G-PERF |
| 375 | LIN-0320 | [D08](#d08) | Измерить scalar access dispatch | PERF | candidate | M | G-PERF |
| 388 | LIN-0333 | [D09](#d09) | Измерить arena для temporary keys | PERF | candidate | M | G-PERF |
| 389 | LIN-0334 | [D09](#d09) | Измерить arena для plan scratch | PERF | candidate | M | G-PERF |
| 390 | LIN-0335 | [D09](#d09) | Измерить arena для join buffers | PERF | candidate | M | G-PERF |
| 391 | LIN-0336 | [D09](#d09) | Измерить arena для FTS candidates | PERF | candidate | M | G-PERF |
| 392 | LIN-0337 | [D09](#d09) | Сравнить Vec recycle policies | PERF | candidate | M | G-PERF |
| 393 | LIN-0338 | [D09](#d09) | Сравнить SmallVec spill distributions | PERF | candidate | L | G-PERF |
| 394 | LIN-0339 | [D09](#d09) | Сравнить buffer shrink thresholds | PERF | candidate | M | G-PERF |
| 395 | LIN-0340 | [D09](#d09) | Сравнить borrowed projection ownership | PERF | candidate | M | G-PERF |
| 396 | LIN-0341 | [D09](#d09) | Профилировать Arc atomic hot spots | PERF | candidate | M | G-PERF |
| 397 | LIN-0342 | [D09](#d09) | Профилировать String key allocation | PERF | candidate | M | G-PERF |
| 398 | LIN-0343 | [D09](#d09) | Профилировать vector Arc allocation | PERF | candidate | M | G-PERF |
| 399 | LIN-0344 | [D09](#d09) | Профилировать metadata clone allocation | PERF | candidate | M | G-PERF |
| 403 | LIN-0369 | [D10](#d10) | Измерить short posting inline storage | PERF | candidate | M | G-PERF |
| 404 | LIN-0370 | [D10](#d10) | Измерить large posting chunking | PERF | candidate | M | G-PERF |
| 405 | LIN-0371 | [D10](#d10) | Измерить sorted bulk index build | PERF | candidate | M | G-PERF |
| 406 | LIN-0372 | [D10](#d10) | Измерить range posting coalescing | PERF | candidate | M | G-PERF |
| 407 | LIN-0373 | [D10](#d10) | Измерить borrowed index bounds | PERF | candidate | M | G-PERF |
| 408 | LIN-0374 | [D10](#d10) | Измерить bounds scratch lifetime | PERF | candidate | M | G-PERF |
| 409 | LIN-0375 | [D10](#d10) | Измерить seek_each allocation count | PERF | candidate | M | G-PERF |
| 410 | LIN-0376 | [D10](#d10) | Измерить extend_into buffer reuse | PERF | candidate | M | G-PERF |
| 419 | LIN-0385 | [D10](#d10) | Спроектировать covering index payload | FEATURE | candidate | L | G-FEATURE |
| 421 | LIN-0387 | [D10](#d10) | Спроектировать expression index contract | FEATURE | candidate | L | G-FEATURE |
| 422 | LIN-0388 | [D10](#d10) | Спроектировать index visibility generations | FEATURE | candidate | L | G-FEATURE |
| 431 | LIN-0397 | [D10](#d10) | Сравнить dense reverse map crossover | PERF | candidate | M | G-PERF |
| 432 | LIN-0398 | [D10](#d10) | Сравнить adaptive posting representation | PERF | candidate | M | G-PERF |
| 434 | LIN-0400 | [D10](#d10) | Сравнить hash-only equality indexes | PERF | candidate | M | G-PERF |
| 451 | LIN-0421 | [D11](#d11) | Спроектировать named constraints | FEATURE | candidate | L | G-FEATURE |
| 452 | LIN-0422 | [D11](#d11) | Спроектировать configurable NULL distinctness | FEATURE | candidate | L | G-FEATURE |
| 453 | LIN-0423 | [D11](#d11) | Спроектировать CHECK predicates | FEATURE | candidate | L | G-FEATURE |
| 472 | LIN-0458 | [D12](#d12) | Спроектировать named parameter binding | API | candidate | L | G-API |
| 473 | LIN-0459 | [D12](#d12) | Спроектировать batch statement separators | API | candidate | L | G-API |
| 487 | LIN-0493 | [D13](#d13) | Спроектировать nullable schema fields | FEATURE | candidate | L | G-FEATURE |
| 488 | LIN-0494 | [D13](#d13) | Спроектировать union scalar types | FEATURE | candidate | L | G-FEATURE |
| 489 | LIN-0495 | [D13](#d13) | Спроектировать strict schema mode | FEATURE | candidate | L | G-FEATURE |
| 507 | LIN-0513 | [D13](#d13) | Измерить repeated check caching | PERF | candidate | M | G-PERF |
| 508 | LIN-0514 | [D13](#d13) | Измерить catalog lookup interning | PERF | candidate | M | G-PERF |
| 509 | LIN-0515 | [D13](#d13) | Измерить type constraint sharing | PERF | candidate | M | G-PERF |
| 510 | LIN-0516 | [D13](#d13) | Измерить lightweight query validation | PERF | candidate | M | G-PERF |
| 518 | LIN-0528 | [D14](#d14) | Измерить plan-node allocation | PERF | candidate | M | G-PERF |
| 544 | LIN-0554 | [D14](#d14) | Измерить plan cache key construction | PERF | candidate | M | G-PERF |
| 575 | LIN-0601 | [D16](#d16) | Профилировать operator boundary copies | PERF | candidate | M | G-PERF |
| 576 | LIN-0602 | [D16](#d16) | Профилировать residual predicate evaluation | PERF | candidate | M | G-PERF |
| 577 | LIN-0603 | [D16](#d16) | Профилировать row materialization cost | PERF | candidate | M | G-PERF |
| 578 | LIN-0604 | [D16](#d16) | Профилировать projected field access | PERF | candidate | M | G-PERF |
| 579 | LIN-0605 | [D16](#d16) | Измерить fused filter/project | PERF | candidate | M | G-PERF |
| 580 | LIN-0606 | [D16](#d16) | Измерить fused count/filter | PERF | candidate | M | G-PERF |
| 581 | LIN-0607 | [D16](#d16) | Измерить reusable predicate scratch | PERF | candidate | M | G-PERF |
| 582 | LIN-0608 | [D16](#d16) | Измерить field-ID expression access | PERF | candidate | M | G-PERF |
| 587 | LIN-0613 | [D16](#d16) | Спроектировать physical operator interface | FEATURE | candidate | L | G-FEATURE |
| 599 | LIN-0625 | [D16](#d16) | Измерить chunked batch execution | PERF | candidate | M | G-PERF |
| 600 | LIN-0626 | [D16](#d16) | Измерить adaptive batch sizing | PERF | candidate | M | G-PERF |
| 601 | LIN-0627 | [D16](#d16) | Измерить branch-specialized scalar loops | PERF | candidate | M | G-PERF |
| 602 | LIN-0628 | [D16](#d16) | Измерить hot/cold operator separation | PERF | candidate | M | G-PERF |
| 605 | LIN-0631 | [D16](#d16) | Спроектировать execution admission control | FEATURE | candidate | L | G-FEATURE |
| 606 | LIN-0632 | [D16](#d16) | Спроектировать fairness между queries | FEATURE | candidate | L | G-FEATURE |
| 613 | LIN-0639 | [D16](#d16) | Создать operator resource stress suite | PERF | candidate | M | G-PERF |
| 631 | LIN-0657 | [D17](#d17) | Измерить seek cursor opening | PERF | candidate | M | G-PERF |
| 632 | LIN-0658 | [D17](#d17) | Измерить range cursor stepping | PERF | candidate | M | G-PERF |
| 633 | LIN-0659 | [D17](#d17) | Измерить join cursor buffering | PERF | candidate | M | G-PERF |
| 634 | LIN-0660 | [D17](#d17) | Измерить vector cursor buffering | PERF | candidate | M | G-PERF |
| 643 | LIN-0685 | [D18](#d18) | Измерить row-to-column packing | PERF | candidate | M | G-PERF |
| 644 | LIN-0686 | [D18](#d18) | Измерить column-to-row draining | PERF | candidate | M | G-PERF |
| 645 | LIN-0687 | [D18](#d18) | Измерить projected column materialization | PERF | candidate | M | G-PERF |
| 646 | LIN-0688 | [D18](#d18) | Измерить batch ownership transfer | PERF | candidate | M | G-PERF |
| 649 | LIN-0691 | [D18](#d18) | Спроектировать offset-based binary columns | FEATURE | candidate | L | G-FEATURE |
| 659 | LIN-0701 | [D18](#d18) | Измерить SIMD numeric predicates | PERF | candidate | M | G-PERF |
| 660 | LIN-0702 | [D18](#d18) | Измерить SIMD bitmap operations | PERF | candidate | M | G-PERF |
| 661 | LIN-0703 | [D18](#d18) | Измерить packed UTF-8 projection | PERF | candidate | M | G-PERF |
| 662 | LIN-0704 | [D18](#d18) | Измерить vector column alignment | PERF | candidate | M | G-PERF |
| 675 | LIN-0737 | [D19](#d19) | Измерить FxHashMap grouping capacity | PERF | candidate | M | G-PERF |
| 676 | LIN-0738 | [D19](#d19) | Измерить inline group key buffers | PERF | candidate | M | G-PERF |
| 677 | LIN-0739 | [D19](#d19) | Измерить group aggregate state layout | PERF | candidate | M | G-PERF |
| 679 | LIN-0741 | [D19](#d19) | Спроектировать streaming aggregation | FEATURE | candidate | L | G-FEATURE |
| 681 | LIN-0743 | [D19](#d19) | Спроектировать sorted-key aggregation | FEATURE | candidate | L | G-FEATURE |
| 703 | LIN-0765 | [D20](#d20) | Измерить decorated sort key reuse | PERF | candidate | M | G-PERF |
| 704 | LIN-0766 | [D20](#d20) | Измерить bounded Top-K crossover | PERF | candidate | M | G-PERF |
| 705 | LIN-0767 | [D20](#d20) | Измерить partial selection partition | PERF | candidate | M | G-PERF |
| 706 | LIN-0768 | [D20](#d20) | Измерить multi-key comparator cost | PERF | candidate | M | G-PERF |
| 728 | LIN-0806 | [D21](#d21) | Профилировать hash table build cost | PERF | candidate | M | G-PERF |
| 729 | LIN-0807 | [D21](#d21) | Профилировать join result materialization | PERF | candidate | M | G-PERF |
| 730 | LIN-0808 | [D21](#d21) | Профилировать semi-join prefilter | PERF | candidate | M | G-PERF |
| 731 | LIN-0809 | [D21](#d21) | Измерить key-only join build | PERF | candidate | M | G-PERF |
| 732 | LIN-0810 | [D21](#d21) | Измерить borrowed join keys | PERF | candidate | M | G-PERF |
| 733 | LIN-0811 | [D21](#d21) | Измерить compact join payloads | PERF | candidate | M | G-PERF |
| 734 | LIN-0812 | [D21](#d21) | Измерить dense ID joins | PERF | candidate | M | G-PERF |
| 778 | LIN-0893 | [D23](#d23) | Измерить ASCII token fast path | PERF | candidate | M | G-PERF |
| 779 | LIN-0894 | [D23](#d23) | Измерить byte-scan tokenizer crossover | PERF | candidate | M | G-PERF |
| 780 | LIN-0895 | [D23](#d23) | Измерить borrowed token lifetimes | PERF | candidate | M | G-PERF |
| 781 | LIN-0896 | [D23](#d23) | Измерить interned term dictionaries | PERF | candidate | M | G-PERF |
| 790 | LIN-0905 | [D23](#d23) | Измерить parallel FTS build | PERF | candidate | M | G-PERF |
| 791 | LIN-0906 | [D23](#d23) | Измерить sorted term bulk build | PERF | candidate | M | G-PERF |
| 792 | LIN-0907 | [D23](#d23) | Измерить posting delta compression | PERF | candidate | M | G-PERF |
| 793 | LIN-0908 | [D23](#d23) | Измерить adaptive posting container | PERF | candidate | M | G-PERF |
| 812 | LIN-0927 | [D24](#d24) | Измерить bitmap/list adaptive intersection | PERF | candidate | M | G-PERF |
| 813 | LIN-0928 | [D24](#d24) | Измерить query token cache | PERF | candidate | M | G-PERF |
| 821 | LIN-0936 | [D24](#d24) | Спроектировать fuzzy term opt-in | VERIFY | qualification | L | G-VERIFY |
| 838 | LIN-0973 | [D25](#d25) | Измерить pending fold trigger | PERF | candidate | M | G-PERF |
| 839 | LIN-0974 | [D25](#d25) | Измерить incremental fold work | PERF | candidate | M | G-PERF |
| 840 | LIN-0975 | [D25](#d25) | Измерить compaction pause budget | PERF | candidate | M | G-PERF |
| 841 | LIN-0976 | [D25](#d25) | Измерить hot-term mutation cost | PERF | candidate | M | G-PERF |
| 858 | LIN-0993 | [D25](#d25) | Сравнить shared scratch на ASCII | PERF | candidate | M | G-PERF |
| 859 | LIN-0994 | [D25](#d25) | Сравнить shared scratch на Unicode | PERF | candidate | M | G-PERF |
| 860 | LIN-0995 | [D25](#d25) | Сравнить posting delta representation | PERF | candidate | M | G-PERF |
| 861 | LIN-0996 | [D25](#d25) | Сравнить batched term remapping | PERF | candidate | M | G-PERF |
| 866 | LIN-1001 | [D26](#d26) | Профилировать current embed_batch | PERF | candidate | M | G-PERF |
| 867 | LIN-1002 | [D26](#d26) | Профилировать embedding text assembly | PERF | candidate | M | G-PERF |
| 868 | LIN-1003 | [D26](#d26) | Профилировать scratch finish allocation | PERF | candidate | M | G-PERF |
| 869 | LIN-1004 | [D26](#d26) | Профилировать feature hashing instructions | PERF | candidate | M | G-PERF |
| 882 | LIN-1017 | [D26](#d26) | Измерить reusable embedding output buffers | PERF | candidate | M | G-PERF |
| 883 | LIN-1018 | [D26](#d26) | Измерить direct Arc slice construction | PERF | candidate | M | G-PERF |
| 884 | LIN-1019 | [D26](#d26) | Измерить trigram cache working set | PERF | candidate | M | G-PERF |
| 885 | LIN-1020 | [D26](#d26) | Измерить batch-local feature cache | PERF | candidate | M | G-PERF |
| 886 | LIN-1021 | [D26](#d26) | Измерить warm thread-pool embedding | PERF | candidate | M | G-PERF |
| 887 | LIN-1022 | [D26](#d26) | Измерить cold thread-pool startup | PERF | candidate | M | G-PERF |
| 888 | LIN-1023 | [D26](#d26) | Измерить scoped embedding crossover | PERF | candidate | M | G-PERF |
| 889 | LIN-1024 | [D26](#d26) | Измерить concurrent embedding contention | PERF | candidate | M | G-PERF |
| 906 | LIN-1061 | [D27](#d27) | Измерить SQ8 recall degradation | PERF | candidate | M | G-PERF |
| 907 | LIN-1062 | [D27](#d27) | Измерить exact reranking budget | PERF | candidate | M | G-PERF |
| 908 | LIN-1063 | [D27](#d27) | Измерить filtered ANN recall | PERF | candidate | L | G-PERF |
| 909 | LIN-1064 | [D27](#d27) | Измерить skewed-cluster HNSW behavior | PERF | candidate | L | G-PERF |
| 938 | LIN-1133 | [D29](#d29) | Проверить readback map failure | CORRECTNESS | qualification | S | G-CORRECT |
| 939 | LIN-1134 | [D29](#d29) | Проверить mapped buffer cancellation | CORRECTNESS | qualification | S | G-CORRECT |
| 942 | LIN-1137 | [D29](#d29) | Измерить upload/readback split | PERF | candidate | M | G-PERF |
| 943 | LIN-1138 | [D29](#d29) | Измерить resident repeated query benefit | PERF | candidate | M | G-PERF |
| 944 | LIN-1139 | [D29](#d29) | Измерить CPU/GPU crossover cardinality | PERF | candidate | M | G-PERF |
| 945 | LIN-1140 | [D29](#d29) | Измерить GPU command encoding cost | PERF | candidate | M | G-PERF |
| 947 | LIN-1142 | [D29](#d29) | Спроектировать shared resident columns | FEATURE | candidate | L | G-FEATURE |
| 948 | LIN-1143 | [D29](#d29) | Спроектировать resident intermediate batches | FEATURE | candidate | L | G-FEATURE |
| 949 | LIN-1144 | [D29](#d29) | Спроектировать explicit residency handles | FEATURE | candidate | L | G-FEATURE |
| 966 | LIN-1181 | [D30](#d30) | Измерить predicate kernel fusion | PERF | candidate | M | G-PERF |
| 967 | LIN-1182 | [D30](#d30) | Измерить projection kernel fusion | PERF | candidate | M | G-PERF |
| 968 | LIN-1183 | [D30](#d30) | Измерить resident join intermediates | PERF | candidate | M | G-PERF |
| 969 | LIN-1184 | [D30](#d30) | Измерить resident aggregation intermediates | PERF | candidate | M | G-PERF |
| 978 | LIN-1209 | [D31](#d31) | Измерить adjacency map locality | PERF | candidate | M | G-PERF |
| 979 | LIN-1210 | [D31](#d31) | Измерить for_each_neighbor borrowing | PERF | candidate | M | G-PERF |
| 980 | LIN-1211 | [D31](#d31) | Измерить traversal scratch reuse | PERF | candidate | M | G-PERF |
| 981 | LIN-1212 | [D31](#d31) | Измерить sparse visited representation | PERF | candidate | M | G-PERF |
| 994 | LIN-1241 | [D32](#d32) | Профилировать cold page faults | PERF | candidate | M | G-PERF |
| 995 | LIN-1242 | [D32](#d32) | Профилировать cold decode allocations | PERF | candidate | M | G-PERF |
| 996 | LIN-1243 | [D32](#d32) | Профилировать cold projection access | PERF | candidate | M | G-PERF |
| 997 | LIN-1244 | [D32](#d32) | Профилировать cold index rebuilding | PERF | candidate | M | G-PERF |
| 1002 | LIN-1249 | [D32](#d32) | Измерить sequential madvise policy | PERF | candidate | M | G-PERF |
| 1003 | LIN-1250 | [D32](#d32) | Измерить random madvise policy | PERF | candidate | M | G-PERF |
| 1004 | LIN-1251 | [D32](#d32) | Измерить bounded prefetch windows | PERF | candidate | M | G-PERF |
| 1005 | LIN-1252 | [D32](#d32) | Измерить page-aligned section layout | PERF | candidate | M | G-PERF |
| 1014 | LIN-1261 | [D32](#d32) | Измерить dictionary cold strings | PERF | candidate | M | G-PERF |
| 1015 | LIN-1262 | [D32](#d32) | Измерить delta-encoded cold integers | PERF | candidate | M | G-PERF |
| 1016 | LIN-1263 | [D32](#d32) | Измерить compressed cold vectors | PERF | candidate | M | G-PERF |
| 1017 | LIN-1264 | [D32](#d32) | Измерить compressed block random access | PERF | candidate | M | G-PERF |
| 1030 | LIN-1293 | [D33](#d33) | Измерить first-item async latency | PERF | candidate | M | G-PERF |
| 1031 | LIN-1294 | [D33](#d33) | Измерить async batch coalescing | PERF | candidate | M | G-PERF |
| 1032 | LIN-1295 | [D33](#d33) | Измерить scheduler handoff overhead | PERF | candidate | M | G-PERF |
| 1033 | LIN-1296 | [D33](#d33) | Измерить stream wakeup amplification | PERF | candidate | M | G-PERF |
| 1046 | LIN-1329 | [D34](#d34) | Измерить read snapshot creation | PERF | candidate | M | G-PERF |
| 1047 | LIN-1330 | [D34](#d34) | Измерить snapshot retained memory | PERF | candidate | M | G-PERF |
| 1048 | LIN-1331 | [D34](#d34) | Измерить Arc contention concurrent readers | PERF | candidate | M | G-PERF |
| 1049 | LIN-1332 | [D34](#d34) | Измерить cross-thread query scaling | PERF | candidate | M | G-PERF |
| 1063 | LIN-1366 | [D35](#d35) | Измерить single-write handshake cost | PERF | candidate | M | G-PERF |
| 1064 | LIN-1367 | [D35](#d35) | Измерить replication RTT budget | PERF | candidate | M | G-PERF |
| 1065 | LIN-1368 | [D35](#d35) | Измерить replication throughput saturation | PERF | candidate | M | G-PERF |
| 1070 | LIN-1373 | [D35](#d35) | Спроектировать token rotation overlap | FEATURE | candidate | L | G-FEATURE |
| 1071 | LIN-1374 | [D35](#d35) | Спроектировать replication authorization scopes | FEATURE | candidate | L | G-FEATURE |
| 1072 | LIN-1375 | [D35](#d35) | Спроектировать authenticated peer identity | FEATURE | candidate | L | G-FEATURE |
| 1073 | LIN-1376 | [D35](#d35) | Спроектировать replay-resistant session nonce | FEATURE | candidate | L | G-FEATURE |
| 1094 | LIN-1417 | [D36](#d36) | Проверить follower disk-full behavior | CORRECTNESS | qualification | S | G-CORRECT |
| 1250 | LIN-1693 | [D43](#d43) | Инъецировать ENOSPC при append | VERIFY | qualification | S | G-VERIFY |
| 1251 | LIN-1694 | [D43](#d43) | Инъецировать EIO при sync | VERIFY | qualification | S | G-VERIFY |
| 1253 | LIN-1696 | [D43](#d43) | Инъецировать EMFILE при reopen | VERIFY | qualification | S | G-VERIFY |
| 1254 | LIN-1697 | [D43](#d43) | Инъецировать network drop при bootstrap | VERIFY | qualification | S | G-VERIFY |
| 1255 | LIN-1698 | [D43](#d43) | Инъецировать network drop при catch-up | VERIFY | qualification | S | G-VERIFY |
| 1256 | LIN-1699 | [D43](#d43) | Инъецировать TLS failure при reconnect | VERIFY | qualification | S | G-VERIFY |
| 1257 | LIN-1700 | [D43](#d43) | Инъецировать slow replica acknowledgement | VERIFY | qualification | S | G-VERIFY |
| 1381 | LIN-1925 | [D49](#d49) | Измерить default build time | PERF | candidate | M | G-PERF |
| 1382 | LIN-1926 | [D49](#d49) | Измерить incremental build time | PERF | candidate | M | G-PERF |
| 1383 | LIN-1927 | [D49](#d49) | Измерить release link time | PERF | candidate | M | G-PERF |
| 1384 | LIN-1928 | [D49](#d49) | Измерить bundled DuckDB build footprint | PERF | candidate | M | G-PERF |

#### P3: 120 пунктов

| Rank | ID | Область | Задача | Тип | Статус | Объём | Gate |
|---:|---|---|---|---|---|---|---|
| 1409 | LIN-0193 | [D05](#d05) | Сравнить cold snapshot formats | PERF | candidate | M | G-PERF |
| 1410 | LIN-0194 | [D05](#d05) | Сравнить snapshot encode chunk sizes | PERF | candidate | M | G-PERF |
| 1412 | LIN-0196 | [D05](#d05) | Сравнить dictionary text snapshots | PERF | candidate | M | G-PERF |
| 1421 | LIN-0349 | [D09](#d09) | Ввести per-query allocation budget | PERF | candidate | M | G-PERF |
| 1422 | LIN-0350 | [D09](#d09) | Ввести per-plan retained-memory estimate | PERF | candidate | M | G-PERF |
| 1425 | LIN-0353 | [D09](#d09) | Измерить allocator optional feature | PERF | candidate | M | G-PERF |
| 1426 | LIN-0354 | [D09](#d09) | Измерить allocator cold startup | PERF | candidate | M | G-PERF |
| 1427 | LIN-0355 | [D09](#d09) | Измерить allocator fragmentation churn | PERF | candidate | M | G-PERF |
| 1428 | LIN-0356 | [D09](#d09) | Измерить allocator multithread contention | PERF | candidate | M | G-PERF |
| 1429 | LIN-0357 | [D09](#d09) | Проверить OOM error boundaries | CORRECTNESS | qualification | S | G-CORRECT |
| 1437 | LIN-0429 | [D11](#d11) | Спроектировать constraint validation mode | FEATURE | candidate | L | G-FEATURE |
| 1438 | LIN-0430 | [D11](#d11) | Спроектировать migration constraint staging | FEATURE | candidate | L | G-FEATURE |
| 1440 | LIN-0432 | [D11](#d11) | Спроектировать validation progress receipts | FEATURE | candidate | L | G-FEATURE |
| 1449 | LIN-0465 | [D12](#d12) | Сократить temporary lexer strings | PERF | candidate | M | G-PERF |
| 1450 | LIN-0466 | [D12](#d12) | Измерить zero-copy token spans | PERF | candidate | M | G-PERF |
| 1451 | LIN-0467 | [D12](#d12) | Измерить keyword dispatch table | PERF | candidate | M | G-PERF |
| 1452 | LIN-0468 | [D12](#d12) | Измерить parser arena allocation | PERF | candidate | M | G-PERF |
| 1489 | LIN-0673 | [D17](#d17) | Измерить async stream chunk sizes | PERF | candidate | M | G-PERF |
| 1490 | LIN-0674 | [D17](#d17) | Измерить cursor to RecordBatch path | PERF | candidate | M | G-PERF |
| 1491 | LIN-0675 | [D17](#d17) | Измерить cursor iterator fusion | PERF | candidate | M | G-PERF |
| 1492 | LIN-0676 | [D17](#d17) | Измерить cursor field-slot cache | PERF | candidate | M | G-PERF |
| 1513 | LIN-0785 | [D20](#d20) | Измерить indirect row-index sorting | PERF | candidate | M | G-PERF |
| 1514 | LIN-0786 | [D20](#d20) | Измерить inline sort keys | PERF | candidate | M | G-PERF |
| 1515 | LIN-0787 | [D20](#d20) | Измерить column sort permutation | PERF | candidate | M | G-PERF |
| 1516 | LIN-0788 | [D20](#d20) | Измерить sorting string dictionaries | PERF | candidate | M | G-PERF |
| 1525 | LIN-0797 | [D20](#d20) | Сравнить ordered-index scan с sort | PERF | candidate | M | G-PERF |
| 1526 | LIN-0798 | [D20](#d20) | Сравнить prefix-sorted execution | PERF | candidate | M | G-PERF |
| 1527 | LIN-0799 | [D20](#d20) | Сравнить radix sort для integers | PERF | candidate | M | G-PERF |
| 1528 | LIN-0800 | [D20](#d20) | Сравнить adaptive small-array sorting | PERF | candidate | M | G-PERF |
| 1529 | LIN-0825 | [D21](#d21) | Измерить selective-side prefilter | PERF | candidate | M | G-PERF |
| 1530 | LIN-0826 | [D21](#d21) | Измерить bitmap semi-join filter | PERF | candidate | M | G-PERF |
| 1531 | LIN-0827 | [D21](#d21) | Измерить bloom join prefilter | PERF | candidate | M | G-PERF |
| 1532 | LIN-0828 | [D21](#d21) | Измерить adaptive join crossover | PERF | candidate | M | G-PERF |
| 1545 | LIN-0865 | [D22](#d22) | Измерить reusable partition buffers | PERF | candidate | M | G-PERF |
| 1546 | LIN-0866 | [D22](#d22) | Измерить sorted-partition reuse | PERF | candidate | M | G-PERF |
| 1547 | LIN-0867 | [D22](#d22) | Измерить multiple-window sharing | PERF | candidate | M | G-PERF |
| 1548 | LIN-0868 | [D22](#d22) | Измерить monotonic deque min/max | PERF | candidate | M | G-PERF |
| 1561 | LIN-0945 | [D24](#d24) | Измерить bounded candidate heaps | PERF | candidate | M | G-PERF |
| 1562 | LIN-0946 | [D24](#d24) | Измерить score-only cursor path | PERF | candidate | M | G-PERF |
| 1563 | LIN-0947 | [D24](#d24) | Измерить query-time allocation count | PERF | candidate | M | G-PERF |
| 1564 | LIN-0948 | [D24](#d24) | Измерить post-filter versus prefilter | PERF | candidate | M | G-PERF |
| 1593 | LIN-1065 | [D27](#d27) | Спроектировать background ANN rebuild | FEATURE | candidate | L | G-FEATURE |
| 1594 | LIN-1066 | [D27](#d27) | Спроектировать bounded ANN rebuild memory | FEATURE | candidate | L | G-FEATURE |
| 1595 | LIN-1067 | [D27](#d27) | Спроектировать ANN generation swaps | FEATURE | candidate | L | G-FEATURE |
| 1605 | LIN-1077 | [D27](#d27) | Сравнить HNSW с exact small-N | PERF | candidate | L | G-PERF |
| 1606 | LIN-1078 | [D27](#d27) | Сравнить graph degree crossover | PERF | candidate | M | G-PERF |
| 1607 | LIN-1079 | [D27](#d27) | Сравнить quantized/nonquantized candidates | PERF | candidate | M | G-PERF |
| 1608 | LIN-1080 | [D27](#d27) | Сравнить adaptive reranking thresholds | PERF | candidate | M | G-PERF |
| 1609 | LIN-1105 | [D28](#d28) | Измерить dense cosine ILP | PERF | candidate | M | G-PERF |
| 1610 | LIN-1106 | [D28](#d28) | Измерить vector norm caching | PERF | candidate | M | G-PERF |
| 1611 | LIN-1107 | [D28](#d28) | Измерить SIMD exact cosine | PERF | candidate | M | G-PERF |
| 1612 | LIN-1108 | [D28](#d28) | Измерить sparse cosine crossover | PERF | candidate | M | G-PERF |
| 1613 | LIN-1109 | [D28](#d28) | Проверить ONNX model output dimensions | CORRECTNESS | qualification | S | G-CORRECT |
| 1614 | LIN-1110 | [D28](#d28) | Проверить Ollama model output ordering | CORRECTNESS | qualification | S | G-CORRECT |
| 1615 | LIN-1111 | [D28](#d28) | Проверить remote embedder failure cleanup | CORRECTNESS | qualification | S | G-CORRECT |
| 1619 | LIN-1115 | [D28](#d28) | Ограничить neural embedding concurrency | CORRECTNESS | qualification | M | G-CORRECT |
| 1620 | LIN-1116 | [D28](#d28) | Ограничить embedding RPC timeout | CORRECTNESS | qualification | M | G-CORRECT |
| 1634 | LIN-1154 | [D29](#d29) | Проверить Vulkan backend conformance | CORRECTNESS | qualification | S | G-CORRECT |
| 1635 | LIN-1155 | [D29](#d29) | Проверить DX12 backend conformance | CORRECTNESS | qualification | S | G-CORRECT |
| 1637 | LIN-1157 | [D29](#d29) | Спроектировать GPU admission control | FEATURE | candidate | L | G-FEATURE |
| 1638 | LIN-1158 | [D29](#d29) | Спроектировать fair multi-client queue | FEATURE | candidate | L | G-FEATURE |
| 1639 | LIN-1159 | [D29](#d29) | Спроектировать query cancellation fences | FEATURE | candidate | L | G-FEATURE |
| 1649 | LIN-1193 | [D30](#d30) | Измерить small dispatch batching | PERF | candidate | M | G-PERF |
| 1650 | LIN-1194 | [D30](#d30) | Измерить workgroup size tuning | PERF | candidate | M | G-PERF |
| 1651 | LIN-1195 | [D30](#d30) | Измерить alignment-aware packed columns | PERF | candidate | M | G-PERF |
| 1652 | LIN-1196 | [D30](#d30) | Измерить transfer-free scalar result path | PERF | candidate | M | G-PERF |
| 1661 | LIN-1229 | [D31](#d31) | Спроектировать graph snapshot cursors | FEATURE | candidate | L | G-FEATURE |
| 1662 | LIN-1230 | [D31](#d31) | Спроектировать streaming path results | FEATURE | candidate | L | G-FEATURE |
| 1664 | LIN-1232 | [D31](#d31) | Спроектировать traversal continuation tokens | FEATURE | candidate | L | G-FEATURE |
| 1685 | LIN-1277 | [D32](#d32) | Проверить Linux mmap behavior | CORRECTNESS | qualification | S | G-CORRECT |
| 1687 | LIN-1279 | [D32](#d32) | Проверить Windows mapped-file behavior | CORRECTNESS | qualification | S | G-CORRECT |
| 1688 | LIN-1280 | [D32](#d32) | Проверить network-backed cold policy | CORRECTNESS | qualification | S | G-CORRECT |
| 1713 | LIN-1353 | [D34](#d34) | Сравнить RwLock snapshot publication | PERF | candidate | M | G-PERF |
| 1714 | LIN-1354 | [D34](#d34) | Сравнить atomic Arc publication | PERF | candidate | M | G-PERF |
| 1715 | LIN-1355 | [D34](#d34) | Сравнить copy-on-write metadata | PERF | candidate | M | G-PERF |
| 1716 | LIN-1356 | [D34](#d34) | Сравнить per-query borrowed metadata | PERF | candidate | M | G-PERF |
| 1717 | LIN-1357 | [D34](#d34) | Добавить loom publication models | FEATURE | candidate | M | G-FEATURE |
| 1725 | LIN-1389 | [D35](#d35) | Спроектировать compressed transport negotiation | FEATURE | candidate | L | G-FEATURE |
| 1726 | LIN-1390 | [D35](#d35) | Спроектировать streamed backup transfer | FEATURE | candidate | L | G-FEATURE |
| 1733 | LIN-1397 | [D35](#d35) | Добавить network fault replay corpus | VERIFY | qualification | S | G-VERIFY |
| 1736 | LIN-1400 | [D35](#d35) | Добавить loopback latency benchmarks | FEATURE | candidate | M | G-FEATURE |
| 1745 | LIN-1433 | [D36](#d36) | Измерить live-query catch-up impact | PERF | candidate | M | G-PERF |
| 1746 | LIN-1434 | [D36](#d36) | Измерить replay/index pipeline crossover | PERF | candidate | M | G-PERF |
| 1747 | LIN-1435 | [D36](#d36) | Измерить batched follower fsync | PERF | candidate | M | G-PERF |
| 1748 | LIN-1436 | [D36](#d36) | Измерить initial bootstrap RSS | PERF | candidate | M | G-PERF |
| 1749 | LIN-1437 | [D36](#d36) | Добавить follower chaos scenarios | FEATURE | candidate | M | G-FEATURE |
| 1777 | LIN-1513 | [D38](#d38) | Измерить typed row decode overhead | PERF | candidate | M | G-PERF |
| 1778 | LIN-1514 | [D38](#d38) | Измерить derive-generated allocation count | PERF | candidate | M | G-PERF |
| 1779 | LIN-1515 | [D38](#d38) | Измерить typed batch encode throughput | PERF | candidate | M | G-PERF |
| 1780 | LIN-1516 | [D38](#d38) | Измерить schema descriptor reuse | PERF | candidate | M | G-PERF |
| 1795 | LIN-1555 | [D39](#d39) | Проверить Windows path arguments | CORRECTNESS | qualification | S | G-CORRECT |
| 1809 | LIN-1593 | [D40](#d40) | Измерить cold error path allocation | PERF | candidate | M | G-PERF |
| 1810 | LIN-1594 | [D40](#d40) | Измерить success-path diagnostic overhead | PERF | candidate | M | G-PERF |
| 1811 | LIN-1595 | [D40](#d40) | Измерить verbose trace opt-in cost | PERF | candidate | M | G-PERF |
| 1812 | LIN-1596 | [D40](#d40) | Измерить batch error aggregation cost | PERF | candidate | M | G-PERF |
| 1849 | LIN-1705 | [D43](#d43) | Спроектировать pluggable filesystem shim | VERIFY | qualification | L | G-VERIFY |
| 1851 | LIN-1707 | [D43](#d43) | Спроектировать crash subprocess harness | VERIFY | qualification | L | G-VERIFY |
| 1854 | LIN-1710 | [D43](#d43) | Проверить sync failure на group commit | VERIFY | qualification | S | G-VERIFY |
| 1856 | LIN-1712 | [D43](#d43) | Проверить failed directory sync recovery | VERIFY | qualification | S | G-VERIFY |
| 1859 | LIN-1715 | [D43](#d43) | Добавить replication chaos matrix | VERIFY | qualification | S | G-VERIFY |
| 1860 | LIN-1716 | [D43](#d43) | Добавить cross-version crash fixtures | VERIFY | qualification | S | G-VERIFY |
| 1901 | LIN-1829 | [D46](#d46) | Измерить catalog clone footprint | PERF | candidate | M | G-PERF |
| 1902 | LIN-1830 | [D46](#d46) | Измерить interned catalog identifiers | PERF | candidate | M | G-PERF |
| 1903 | LIN-1831 | [D46](#d46) | Измерить schema lookup cache | PERF | candidate | M | G-PERF |
| 1904 | LIN-1832 | [D46](#d46) | Измерить prepared schema invalidation | PERF | candidate | M | G-PERF |
| 1921 | LIN-1873 | [D47](#d47) | Измерить columnar import throughput | PERF | candidate | M | G-PERF |
| 1922 | LIN-1874 | [D47](#d47) | Измерить CSV parser allocation | PERF | candidate | M | G-PERF |
| 1923 | LIN-1875 | [D47](#d47) | Измерить large export memory bound | PERF | candidate | M | G-PERF |
| 1924 | LIN-1876 | [D47](#d47) | Измерить import index maintenance cost | PERF | candidate | M | G-PERF |
| 1929 | LIN-1905 | [D48](#d48) | Измерить open/close churn | PERF | candidate | M | G-PERF |
| 1930 | LIN-1906 | [D48](#d48) | Измерить long-running RSS growth | PERF | candidate | M | G-PERF |
| 1931 | LIN-1907 | [D48](#d48) | Измерить repeated checkpoint fragmentation | PERF | candidate | M | G-PERF |
| 1932 | LIN-1908 | [D48](#d48) | Измерить repeated replica reconnect leakage | PERF | candidate | M | G-PERF |
| 1949 | LIN-1937 | [D49](#d49) | Измерить binary size sections | PERF | candidate | M | G-PERF |
| 1950 | LIN-1938 | [D49](#d49) | Измерить LTO variants | PERF | candidate | M | G-PERF |
| 1951 | LIN-1939 | [D49](#d49) | Измерить codegen-units variants | PERF | candidate | M | G-PERF |
| 1952 | LIN-1940 | [D49](#d49) | Измерить PGO workload sensitivity | PERF | candidate | M | G-PERF |
| 1961 | LIN-1949 | [D49](#d49) | Спроектировать native Python wheel matrix | OPS | candidate | L | G-OPS |
| 1962 | LIN-1950 | [D49](#d49) | Спроектировать C library packaging | OPS | candidate | L | G-OPS |
| 1963 | LIN-1951 | [D49](#d49) | Спроектировать WASM package split | OPS | candidate | L | G-OPS |

### Области и привязка к коду

Зависимости областей — контекст для чтения, а не требование закончить одну область перед другой.

<a id="d01"></a>

**D01. Бенчмарки и доказательства** — открыто 27 из 40. Модули: [benches/compare.rs](../benches/compare.rs), [scripts/bench-peers.py](../scripts/bench-peers.py), [docs/benchmark-contract.md](../docs/benchmark-contract.md).

<a id="d02"></a>

**D02. Формат WAL и checksum** — открыто 4 из 40. Модули: [src/persist/mod.rs](../src/persist/mod.rs), [src/crc.rs](../src/crc.rs), [tests/persist.rs](../tests/persist.rs). Контекст: [D01](#d01).

<a id="d03"></a>

**D03. Durable I/O и sync** — открыто 15 из 40. Модули: [src/persist/mod.rs](../src/persist/mod.rs), [src/ship.rs](../src/ship.rs), [tests/persist.rs](../tests/persist.rs). Контекст: [D01](#d01), [D02](#d02).

<a id="d04"></a>

**D04. Replay и восстановление** — открыто 4 из 40. Модули: [src/persist/mod.rs](../src/persist/mod.rs), [src/store/mod.rs](../src/store/mod.rs), [tests/persist.rs](../tests/persist.rs). Контекст: [D02](#d02), [D03](#d03).

<a id="d05"></a>

**D05. Snapshot и checkpoint** — открыто 6 из 40. Модули: [src/persist/mod.rs](../src/persist/mod.rs), [src/store/mod.rs](../src/store/mod.rs), [src/disk/mod.rs](../src/disk/mod.rs). Контекст: [D02](#d02), [D03](#d03), [D04](#d04).

<a id="d06"></a>

**D06. Транзакции и atomicity** — открыто 2 из 40. Модули: [src/exec/mod.rs](../src/exec/mod.rs), [src/store/mod.rs](../src/store/mod.rs), [tests/exec.rs](../tests/exec.rs). Контекст: [D02](#d02), [D04](#d04).

<a id="d07"></a>

**D07. Mutations и bulk ingestion** — открыто 12 из 40. Модули: [src/exec/mod.rs](../src/exec/mod.rs), [src/store/mod.rs](../src/store/mod.rs), [tests/exec.rs](../tests/exec.rs). Контекст: [D06](#d06), [D08](#d08), [D10](#d10).

<a id="d08"></a>

**D08. Cell и строковое представление** — открыто 15 из 40. Модули: [src/store/mod.rs](../src/store/mod.rs), [src/row.rs](../src/row.rs), [src/ast.rs](../src/ast.rs). Контекст: [D01](#d01).

<a id="d09"></a>

**D09. Владение и пулы памяти** — открыто 19 из 40. Модули: [src/row.rs](../src/row.rs), [src/batch.rs](../src/batch.rs), [src/cursor.rs](../src/cursor.rs). Контекст: [D08](#d08).

<a id="d10"></a>

**D10. Scalar indexes** — открыто 14 из 40. Модули: [src/index.rs](../src/index.rs), [src/store/mod.rs](../src/store/mod.rs), [tests/query.rs](../tests/query.rs). Контекст: [D08](#d08), [D11](#d11).

<a id="d11"></a>

**D11. Ограничения и числовые ключи** — открыто 6 из 40. Модули: [src/index.rs](../src/index.rs), [src/check.rs](../src/check.rs), [tests/exec.rs](../tests/exec.rs). Контекст: [D08](#d08), [D13](#d13).

<a id="d12"></a>

**D12. Парсер и синтаксис** — открыто 6 из 40. Модули: [src/parse.rs](../src/parse.rs), [src/ast.rs](../src/ast.rs), [tests/lang.rs](../tests/lang.rs). Контекст: [D13](#d13).

<a id="d13"></a>

**D13. Type checking и semantics** — открыто 7 из 40. Модули: [src/check.rs](../src/check.rs), [src/catalog.rs](../src/catalog.rs), [tests/lang.rs](../tests/lang.rs). Контекст: [D08](#d08), [D46](#d46).

<a id="d14"></a>

**D14. Логический planner** — открыто 2 из 40. Модули: [src/plan.rs](../src/plan.rs), [src/explain.rs](../src/explain.rs), [tests/query.rs](../tests/query.rs). Контекст: [D13](#d13).

<a id="d15"></a>

**D15. Статистика и cost model** — открыто 0 из 40. Модули: [src/plan.rs](../src/plan.rs), [src/catalog.rs](../src/catalog.rs), [src/index.rs](../src/index.rs). Контекст: [D14](#d14), [D10](#d10).

<a id="d16"></a>

**D16. Execution pipeline** — открыто 16 из 40. Модули: [src/exec/mod.rs](../src/exec/mod.rs), [src/row.rs](../src/row.rs), [src/batch.rs](../src/batch.rs). Контекст: [D13](#d13), [D14](#d14).

<a id="d17"></a>

**D17. Cursors и streaming** — открыто 8 из 40. Модули: [src/cursor.rs](../src/cursor.rs), [src/query.rs](../src/query.rs), [src/async_db.rs](../src/async_db.rs). Контекст: [D16](#d16), [D34](#d34).

<a id="d18"></a>

**D18. Колонки и RecordBatch** — открыто 9 из 40. Модули: [src/batch.rs](../src/batch.rs), [src/row.rs](../src/row.rs), [src/query.rs](../src/query.rs). Контекст: [D08](#d08).

<a id="d19"></a>

**D19. Aggregations и GROUP BY** — открыто 5 из 40. Модули: [src/exec/mod.rs](../src/exec/mod.rs), [src/store/mod.rs](../src/store/mod.rs), [tests/query.rs](../tests/query.rs). Контекст: [D08](#d08), [D13](#d13).

<a id="d20"></a>

**D20. Sorting и Top-K** — открыто 12 из 40. Модули: [src/exec/mod.rs](../src/exec/mod.rs), [src/cursor.rs](../src/cursor.rs), [src/store/mod.rs](../src/store/mod.rs). Контекст: [D08](#d08), [D13](#d13).

<a id="d21"></a>

**D21. Joins и relational composition** — открыто 11 из 40. Модули: [src/exec/mod.rs](../src/exec/mod.rs), [src/cursor.rs](../src/cursor.rs), [src/batch.rs](../src/batch.rs). Контекст: [D08](#d08), [D14](#d14).

<a id="d22"></a>

**D22. Window functions** — открыто 4 из 40. Модули: [src/window.rs](../src/window.rs), [src/exec/mod.rs](../src/exec/mod.rs), [tests/query.rs](../tests/query.rs). Контекст: [D08](#d08), [D19](#d19), [D20](#d20).

<a id="d23"></a>

**D23. FTS build и tokenizer** — открыто 8 из 40. Модули: [src/fts.rs](../src/fts.rs), [src/catalog.rs](../src/catalog.rs), [tests/query.rs](../tests/query.rs). Контекст: [D08](#d08), [D46](#d46).

<a id="d24"></a>

**D24. FTS query и scoring** — открыто 7 из 40. Модули: [src/fts.rs](../src/fts.rs), [src/exec/mod.rs](../src/exec/mod.rs), [src/cursor.rs](../src/cursor.rs). Контекст: [D23](#d23).

<a id="d25"></a>

**D25. FTS mutation и compaction** — открыто 8 из 40. Модули: [src/fts.rs](../src/fts.rs), [src/store/mod.rs](../src/store/mod.rs), [tests/delete_readback.rs](../tests/delete_readback.rs). Контекст: [D06](#d06), [D23](#d23).

<a id="d26"></a>

**D26. Hashing embeddings** — открыто 12 из 40. Модули: [src/embed.rs](../src/embed.rs), [src/exec/mod.rs](../src/exec/mod.rs), [tests/embed.rs](../tests/embed.rs). Контекст: [D08](#d08), [D13](#d13).

<a id="d27"></a>

**D27. ANN и HNSW** — открыто 11 из 40. Модули: [src/embed.rs](../src/embed.rs), [src/store/mod.rs](../src/store/mod.rs), [src/exec/mod.rs](../src/exec/mod.rs). Контекст: [D26](#d26), [D28](#d28).

<a id="d28"></a>

**D28. Vector search semantics** — открыто 9 из 40. Модули: [src/embed.rs](../src/embed.rs), [src/exec/mod.rs](../src/exec/mod.rs), [src/cursor.rs](../src/cursor.rs). Контекст: [D26](#d26).

<a id="d29"></a>

**D29. GPU resources и residency** — открыто 14 из 40. Модули: [src/gpu.rs](../src/gpu.rs), [src/gpu_regex.rs](../src/gpu_regex.rs), [tests/gpu.rs](../tests/gpu.rs). Контекст: [D18](#d18).

<a id="d30"></a>

**D30. GPU kernels и equivalence** — открыто 8 из 40. Модули: [src/gpu.rs](../src/gpu.rs), [src/shaders/compare.wgsl](../src/shaders/compare.wgsl), [tests/shaders.rs](../tests/shaders.rs). Контекст: [D29](#d29).

<a id="d31"></a>

**D31. Graph queries** — открыто 7 из 40. Модули: [src/graph.rs](../src/graph.rs), [src/store/mod.rs](../src/store/mod.rs), [src/cursor.rs](../src/cursor.rs). Контекст: [D08](#d08), [D06](#d06).

<a id="d32"></a>

**D32. Cold storage и mmap** — открыто 15 из 40. Модули: [src/disk/mod.rs](../src/disk/mod.rs), [src/store/mod.rs](../src/store/mod.rs), [src/persist/mod.rs](../src/persist/mod.rs). Контекст: [D04](#d04), [D05](#d05).

<a id="d33"></a>

**D33. Async execution и backpressure** — открыто 4 из 40. Модули: [src/async_db.rs](../src/async_db.rs), [src/query.rs](../src/query.rs), [tests/query.rs](../tests/query.rs). Контекст: [D16](#d16), [D34](#d34).

<a id="d34"></a>

**D34. Concurrency и read snapshots** — открыто 9 из 40. Модули: [src/exec/mod.rs](../src/exec/mod.rs), [src/store/mod.rs](../src/store/mod.rs), [src/async_db.rs](../src/async_db.rs). Контекст: [D06](#d06).

<a id="d35"></a>

**D35. Replication transport** — открыто 11 из 40. Модули: [src/ship.rs](../src/ship.rs), [src/main.rs](../src/main.rs), [tests/persist.rs](../tests/persist.rs). Контекст: [D02](#d02), [D04](#d04).

<a id="d36"></a>

**D36. Followers и catch-up** — открыто 6 из 40. Модули: [src/ship.rs](../src/ship.rs), [src/persist/mod.rs](../src/persist/mod.rs), [src/main.rs](../src/main.rs). Контекст: [D35](#d35).

<a id="d37"></a>

**D37. Public Rust API** — открыто 0 из 40. Модули: [src/lib.rs](../src/lib.rs), [src/query.rs](../src/query.rs), [src/exec/mod.rs](../src/exec/mod.rs). Контекст: [D13](#d13).

<a id="d38"></a>

**D38. Typed queries и derive** — открыто 4 из 40. Модули: [lin-derive/src/lib.rs](../lin-derive/src/lib.rs), [src/row.rs](../src/row.rs), [src/query.rs](../src/query.rs). Контекст: [D37](#d37).

<a id="d39"></a>

**D39. CLI и интерактивность** — открыто 1 из 40. Модули: [src/main.rs](../src/main.rs), [src/explain.rs](../src/explain.rs), [tests/compute_cli.rs](../tests/compute_cli.rs). Контекст: [D37](#d37).

<a id="d40"></a>

**D40. Errors и диагностика** — открыто 4 из 40. Модули: [src/error.rs](../src/error.rs), [src/parse.rs](../src/parse.rs), [src/check.rs](../src/check.rs). Контекст: [D13](#d13).

<a id="d41"></a>

**D41. Observability** — открыто 0 из 40. Модули: [src/exec/mod.rs](../src/exec/mod.rs), [src/persist/mod.rs](../src/persist/mod.rs), [src/explain.rs](../src/explain.rs). Контекст: [D40](#d40).

<a id="d42"></a>

**D42. Property и differential testing** — открыто 0 из 40. Модули: [tests/lang.rs](../tests/lang.rs), [tests/query.rs](../tests/query.rs), [tests/persist.rs](../tests/persist.rs). Контекст: [D13](#d13).

<a id="d43"></a>

**D43. Crash и fault injection** — открыто 13 из 40. Модули: [src/persist/mod.rs](../src/persist/mod.rs), [src/ship.rs](../src/ship.rs), [tests/persist.rs](../tests/persist.rs). Контекст: [D02](#d02), [D03](#d03), [D04](#d04).

<a id="d44"></a>

**D44. CI и платформы** — открыто 0 из 40. Модули: [.github/workflows/bench.yml](../.github/workflows/bench.yml), [scripts/ci-bench.sh](../scripts/ci-bench.sh), [tests/shaders.rs](../tests/shaders.rs). Контекст: [D01](#d01), [D42](#d42).

<a id="d45"></a>

**D45. Безопасность и resource limits** — открыто 0 из 40. Модули: [src/exec/mod.rs](../src/exec/mod.rs), [src/persist/mod.rs](../src/persist/mod.rs), [src/ship.rs](../src/ship.rs). Контекст: [D40](#d40).

<a id="d46"></a>

**D46. Catalog и schema evolution** — открыто 4 из 40. Модули: [src/catalog.rs](../src/catalog.rs), [src/check.rs](../src/check.rs), [src/exec/mod.rs](../src/exec/mod.rs). Контекст: [D06](#d06).

<a id="d47"></a>

**D47. Import и export** — открыто 4 из 40. Модули: [src/main.rs](../src/main.rs), [src/row.rs](../src/row.rs), [src/batch.rs](../src/batch.rs). Контекст: [D18](#d18), [D46](#d46).

<a id="d48"></a>

**D48. Operations и lifecycle** — открыто 4 из 40. Модули: [src/main.rs](../src/main.rs), [src/persist/mod.rs](../src/persist/mod.rs), [src/ship.rs](../src/ship.rs). Контекст: [D03](#d03), [D04](#d04), [D05](#d05).

<a id="d49"></a>

**D49. Build и distribution** — открыто 11 из 40. Модули: [Cargo.toml](../Cargo.toml), [Cargo.lock](../Cargo.lock), [src/lib.rs](../src/lib.rs). Контекст: [D44](#d44).

<a id="d50"></a>

**D50. Документация и продуктовый контракт** — открыто 0 из 40. Модули: [README.md](../README.md), [CHANGELOG.md](../CHANGELOG.md), [docs/benchmark-contract.md](../docs/benchmark-contract.md). Контекст: [D01](#d01), [D37](#d37).
