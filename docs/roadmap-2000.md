# Lin — план 2000 улучшений, исправлений и развития

Срез: **2026-10-21**, исходный коммит `1a43ca59bdc3a22e89733091b564b2ecb42e210b`. Ровно **2000 самостоятельных пунктов**, 50 областей, без пропущенных диапазонов. Это предлагаемый backlog, **не 2000 доказанных дефектов и не 2000 обещанных ускорений**. Производственный код этим планом не меняется.

ID `LIN-xxxx` стабилен; `rank` задаёт порядок triage. Явные зависимости выполняются раньше зависимой задачи, даже если их rank ниже. Приоритет — инженерная оценка риска/пользы для текущей цели, а не измеренный рейтинг отдачи. Сначала проверить наличие/корректность функции: уже покрытый пункт закрывается как `covered`, а не реализуется повторно. Дизайн и проверки — отдельные результаты, они не объявляются исправлениями без reproducer.

## Текущая исходная точка

Последние [парные native/durable замеры](../benches/results/2026-10-21-current-native-durable/report.md): native 1k Lin 0,998 мс / SQLite 0,974 мс, 0/6 побед; native 10k — 12,832 / 13,019 мс, 5/6; durable 1k — 2,731 / 2,162 мс, 1/6; durable 10k — 20,716 / 18,092 мс, 1/6. Межпроцессная вариативность велика. Старые матрицы чтения и результаты 6 октября нельзя переносить на изменившийся main без повторного запуска. MSSQL и Kusto не настроены.

В коде уже существуют SSO, HNSW/SQ8, row pooling, hash grouping, hardware CRC32C, streaming cursors, index/FTS fast paths и GPU-вычисления. Их названия в плане означают проверку инвариантов, измерение границ или развитие; не утверждение, что механизм отсутствует. Общая цель — проверяемое превосходство на согласованных сценариях против SQLite, DuckDB, PostgreSQL, MySQL, MSSQL, MongoDB, Kusto и pandas — сохраняется целиком.

## Как исполнять

1. Начать с первых задач ниже; одновременно вести не более одной performance-гипотезы в одном benchmark checkout.
2. Перед изменением записать контракт, источники, baseline, ресурсы и критерий отказа. Пункт `qualification` не предполагает заранее наличия ошибки.
3. Сохранять API/ошибки/порядок/битовую точность там, где они являются контрактом; отдельное изменение семантики требует явного дизайна и миграции.
4. Не ослаблять sync, не отключать embedding/FTS/индексы и не исключать проигрыши ради победы. Новые peer-контракты описывать отдельно.
5. После узкой проверки запускать релевантные интеграционные проверки; ускорение сохранять только после независимого повтора с контролем. Отрицательный результат сохранять как evidence.
6. Блокированные endpoints не считать победами; пароли/токены в roadmap не помещать.
7. После каждого принятого изменения обновлять затронутые измерения и статус задач, не сдвигая стабильные ID.
8. Завершение всего backlog не равно автоматически завершению benchmark-цели: нужен отдельный audit всех named peers и сценариев.

## Очереди исполнения

- **Волна A:** current baseline, CRC/format и Cell Hash/Eq qualification, profiling WAL, unmatched peer contracts.
- **Волна B:** native 1k и durable 1k/10k; сначала текущий профиль, затем по одному кандидату.
- **Волна C:** повторить read matrix и write coverage, подключить MSSQL/Kusto после настройки доступа.
- **Волна D:** ресурсы и корректность indexes/FTS/ANN/cursors/async, differential/crash harness.
- **Волна E:** cost model, spill, schema evolution, расширения API и эксплуатации.
- **Волна F:** внешние bindings, упаковка и широкая platform qualification.

Пункты из разных волн могут понадобиться раньше при подтверждённом дефекте. Здесь нет календарного обещания реализовать все 2000.

## Обозначения

- `P1` — первый triage: текущий bottleneck, совместимость, ресурсы или потенциально значимый инвариант. Не синоним подтверждённого critical bug.
- `P2` — следующий слой развития и квалификации.
- `P3` — дальний backlog, только после проверки полезности и crossover.
- `measured_gap` — конкретный проигрыш из текущего отчёта; `qualification` — ещё не проверенная граница; `candidate` — идея/дизайн; `blocked_external` — требуется внешний доступ.
- `S/M/L` — приблизительный объём исследования, не срок и не измеренная трудоёмкость.
- Связь с файлом означает релевантный модуль для triage, не найденный дефект на каждой его строке.

## Критерии готовности

- **G-PERF** (Оптимизации и измерительные гипотезы): Независимый oracle сохраняет значения/порядок/ошибки; baseline/candidate одного контракта, два набора ≥6 чередующихся process-pairs, контроль SQLite, 1k/10k native+durable при изменении записи; сохранить raw samples/hash/версии/RSS. Смешанный результат не считать ускорением.
- **G-CORRECT** (Корректность и границы): Сначала минимальный reproducer или формальная ambiguity, затем согласованный контракт и независимый oracle. До исправления тест воспроизводит проблему; после исправления проходят релевантные rollback/reopen/replay и границы. Если реализация уже корректна, записать covered, не менять код.
- **G-FEATURE** (Новые функции и дизайн): Сначала проверить наличие в текущем коде. Если отсутствует, описать API/семантику/ресурсные лимиты/ошибки/совместимость; затем сценарий пользователя и end-to-end проверка. Существующую функцию не реализовывать повторно.
- **G-API** (API и типизированные интерфейсы): Публичный пример и compile-time/runtime контракт; проверить downstream использование, стабильные типы, serialization и feature-disabled режим; зафиксировать совместимость и ошибки.
- **G-VERIFY** (Qualification и независимые проверки): Самостоятельный oracle/инвариант, детерминированный seed и минимизируемый fixture; намеренное нарушение ловится проверкой, реальная операция проходит. Сохранить failure artifact; green-only запуск без чувствительности к ошибке недостаточен.
- **G-OPS** (Эксплуатация и инструменты): Воспроизводимый сценарий запуска/отказа/очистки, сохранённые diagnostics и resource bounds. Не менять чужие объекты; секреты не попадают в вывод. Проверить нормальное завершение и отмену.
- **G-DOC** (Документация и доказуемые утверждения): Связать утверждения с текущим API, runnable примером или raw evidence; проверить ссылки и scope. Не публиковать старый результат как текущий или component microbench как скорость всей БД.

Для каждой строки итоговый критерий — её конкретный результат плюс указанный gate. Если критерий требует недоступной платформы/endpoint, статус остаётся непроверенным; mock/compile не заменяет live qualification.

## Первые 25 пунктов

1. **LIN-0001: Закрыть native 1k SQLite gap** — P1, done, G-PERF.
2. **LIN-0002: Закрыть durable 1k SQLite gap** — P1, done, G-PERF.
3. **LIN-0003: Закрыть durable 10k SQLite gap** — P1, done, G-PERF.
4. **LIN-0004: Обновить матрицу чтения текущего main** — P1, candidate, G-PERF.
5. **LIN-0081: Профилировать текущий WAL encode** — P1, done, G-PERF.
6. **LIN-0082: Профилировать текущий WAL pack** — P1, done, G-PERF.
7. **LIN-0084: Профилировать текущий Full-sync** — P1, done, G-PERF.
8. **LIN-0083: Профилировать текущий WAL checksum** — P1, done, G-PERF.
9. **LIN-0281: Зафиксировать Cell equality contract** — P1, done, G-CORRECT.
10. **LIN-0282: Согласовать Cell Hash и Eq** — P1, done, G-CORRECT.
11. **LIN-0302: Проверить mixed numeric hash grouping** — P1, done, G-CORRECT.
12. **LIN-0721: Проверить Cell Hash/Eq для GROUP BY** — P1, done, G-CORRECT.
13. **LIN-0041: Зафиксировать версию checksum envelope** — P1, done, G-CORRECT.
14. **LIN-0042: Обозначить polynomial CRC в формате** — P1, done, G-CORRECT.
15. **LIN-0043: Проверить mixed CRC32/CRC32C replay** — P1, done, G-CORRECT.
16. **LIN-1701: Проверить last-acknowledged prefix recovery** — P1, done, G-VERIFY.
17. **LIN-0367: Проверить unique-conflict before early return** — P1, done, G-CORRECT.
18. **LIN-0886: Проверить implicit snippet indexing** — P1, done, G-CORRECT.
19. **LIN-0029: Сохранять источник измеряемого бинарника** — P1, candidate, G-PERF.
20. **LIN-0030: Проверять source drift после прогона** — P1, candidate, G-PERF.
21. **LIN-0005: Получить тестовый endpoint MSSQL** — P1, blocked_external, G-OPS.
22. **LIN-0006: Получить тестовый endpoint Kusto** — P1, blocked_external, G-OPS.
23. **LIN-0007: Сопоставить transaction boundaries peers** — P1, candidate, G-PERF.
24. **LIN-0008: Сопоставить индексные ограничения peers** — P1, candidate, G-PERF.
25. **LIN-0044: Проверить старый reader на новом WAL** — P1, done, G-CORRECT.

## Полный приоритетный список — все 2000 пунктов


### P1: 110 пунктов

| Rank | ID | Область | Задача | Тип | Статус | Объём | Gate |
|---:|---|---|---|---|---|---|---|
| 1 | LIN-0001 | [D01](#d01) | Закрыть native 1k SQLite gap | PERF | done | M | G-PERF |
| 2 | LIN-0002 | [D01](#d01) | Закрыть durable 1k SQLite gap | PERF | done | M | G-PERF |
| 3 | LIN-0003 | [D01](#d01) | Закрыть durable 10k SQLite gap | PERF | done | M | G-PERF |
| 4 | LIN-0004 | [D01](#d01) | Обновить матрицу чтения текущего main | PERF | candidate | M | G-PERF |
| 5 | LIN-0081 | [D03](#d03) | Профилировать текущий WAL encode | PERF | done | M | G-PERF |
| 6 | LIN-0082 | [D03](#d03) | Профилировать текущий WAL pack | PERF | done | M | G-PERF |
| 7 | LIN-0084 | [D03](#d03) | Профилировать текущий Full-sync | PERF | done | M | G-PERF |
| 8 | LIN-0083 | [D03](#d03) | Профилировать текущий WAL checksum | PERF | done | M | G-PERF |
| 9 | LIN-0281 | [D08](#d08) | Зафиксировать Cell equality contract | CORRECTNESS | done | M | G-CORRECT |
| 10 | LIN-0282 | [D08](#d08) | Согласовать Cell Hash и Eq | CORRECTNESS | done | M | G-CORRECT |
| 11 | LIN-0302 | [D08](#d08) | Проверить mixed numeric hash grouping | CORRECTNESS | done | S | G-CORRECT |
| 12 | LIN-0721 | [D19](#d19) | Проверить Cell Hash/Eq для GROUP BY | CORRECTNESS | done | S | G-CORRECT |
| 13 | LIN-0041 | [D02](#d02) | Зафиксировать версию checksum envelope | CORRECTNESS | done | M | G-CORRECT |
| 14 | LIN-0042 | [D02](#d02) | Обозначить polynomial CRC в формате | CORRECTNESS | done | M | G-CORRECT |
| 15 | LIN-0043 | [D02](#d02) | Проверить mixed CRC32/CRC32C replay | CORRECTNESS | done | S | G-CORRECT |
| 16 | LIN-1701 | [D43](#d43) | Проверить last-acknowledged prefix recovery | VERIFY | done | S | G-VERIFY |
| 17 | LIN-0367 | [D10](#d10) | Проверить unique-conflict before early return | CORRECTNESS | done | S | G-CORRECT |
| 18 | LIN-0886 | [D23](#d23) | Проверить implicit snippet indexing | CORRECTNESS | done | S | G-CORRECT |
| 19 | LIN-0029 | [D01](#d01) | Сохранять источник измеряемого бинарника | PERF | candidate | M | G-PERF |
| 20 | LIN-0030 | [D01](#d01) | Проверять source drift после прогона | PERF | candidate | M | G-PERF |
| 21 | LIN-0005 | [D01](#d01) | Получить тестовый endpoint MSSQL | OPS | blocked_external | M | G-OPS |
| 22 | LIN-0006 | [D01](#d01) | Получить тестовый endpoint Kusto | OPS | blocked_external | M | G-OPS |
| 23 | LIN-0007 | [D01](#d01) | Сопоставить transaction boundaries peers | PERF | candidate | M | G-PERF |
| 24 | LIN-0008 | [D01](#d01) | Сопоставить индексные ограничения peers | PERF | candidate | M | G-PERF |
| 25 | LIN-0044 | [D02](#d02) | Проверить старый reader на новом WAL | CORRECTNESS | done | S | G-CORRECT |
| 26 | LIN-0121 | [D04](#d04) | Проверить torn WAL header | CORRECTNESS | done | S | G-CORRECT |
| 27 | LIN-0122 | [D04](#d04) | Проверить torn WAL body | CORRECTNESS | done | S | G-CORRECT |
| 28 | LIN-0123 | [D04](#d04) | Проверить torn WAL checksum | CORRECTNESS | done | S | G-CORRECT |
| 29 | LIN-0124 | [D04](#d04) | Проверить trailing garbage policy | CORRECTNESS | done | S | G-CORRECT |
| 30 | LIN-0161 | [D05](#d05) | Профилировать snapshot serialization copies | PERF | done | M | G-PERF |
| 31 | LIN-0162 | [D05](#d05) | Профилировать snapshot decode allocations | PERF | done | M | G-PERF |
| 32 | LIN-0163 | [D05](#d05) | Измерить checkpoint pause budget | PERF | candidate | M | G-PERF |
| 33 | LIN-0164 | [D05](#d05) | Измерить checkpoint peak memory | PERF | done | M | G-PERF |
| 34 | LIN-0201 | [D06](#d06) | Зафиксировать transaction visibility contract | CORRECTNESS | qualification | M | G-CORRECT |
| 35 | LIN-0202 | [D06](#d06) | Проверить rollback всех row maps | CORRECTNESS | qualification | S | G-CORRECT |
| 36 | LIN-0203 | [D06](#d06) | Проверить rollback scalar postings | CORRECTNESS | qualification | S | G-CORRECT |
| 37 | LIN-0204 | [D06](#d06) | Проверить rollback FTS pending edits | CORRECTNESS | qualification | S | G-CORRECT |
| 38 | LIN-0241 | [D07](#d07) | Профилировать small bulk row construction | PERF | done | M | G-PERF |
| 39 | LIN-0242 | [D07](#d07) | Профилировать bulk identity checks | PERF | done | M | G-PERF |
| 40 | LIN-0243 | [D07](#d07) | Профилировать bulk FK checks | PERF | done | M | G-PERF |
| 41 | LIN-0244 | [D07](#d07) | Профилировать bulk map registration | PERF | done | M | G-PERF |
| 42 | LIN-0283 | [D08](#d08) | Проверить NaN grouping semantics | CORRECTNESS | done | S | G-CORRECT |
| 43 | LIN-0284 | [D08](#d08) | Проверить signed-zero grouping semantics | CORRECTNESS | done | S | G-CORRECT |
| 44 | LIN-0361 | [D10](#d10) | Профилировать composite key construction | PERF | candidate | M | G-PERF |
| 45 | LIN-0362 | [D10](#d10) | Профилировать index reverse map writes | PERF | candidate | M | G-PERF |
| 46 | LIN-0363 | [D10](#d10) | Профилировать posting list allocations | PERF | candidate | M | G-PERF |
| 47 | LIN-0364 | [D10](#d10) | Профилировать BTree node locality | PERF | candidate | M | G-PERF |
| 48 | LIN-0401 | [D11](#d11) | Проверить int выше 2^53 уникальность | CORRECTNESS | qualification | S | G-CORRECT |
| 49 | LIN-0402 | [D11](#d11) | Проверить float integer tie-breaker | CORRECTNESS | qualification | S | G-CORRECT |
| 50 | LIN-0403 | [D11](#d11) | Проверить negative infinity ordering | CORRECTNESS | qualification | S | G-CORRECT |
| 51 | LIN-0404 | [D11](#d11) | Проверить positive infinity ordering | CORRECTNESS | qualification | S | G-CORRECT |
| 52 | LIN-0481 | [D13](#d13) | Зафиксировать NULL predicate semantics | CORRECTNESS | qualification | M | G-CORRECT |
| 53 | LIN-0482 | [D13](#d13) | Зафиксировать absent-field semantics | CORRECTNESS | qualification | M | G-CORRECT |
| 54 | LIN-0483 | [D13](#d13) | Зафиксировать mixed numeric coercions | CORRECTNESS | qualification | M | G-CORRECT |
| 55 | LIN-0484 | [D13](#d13) | Зафиксировать boolean coercion policy | CORRECTNESS | qualification | M | G-CORRECT |
| 56 | LIN-0521 | [D14](#d14) | Проверить constant folding equivalence | CORRECTNESS | qualification | S | G-CORRECT |
| 57 | LIN-0522 | [D14](#d14) | Проверить predicate simplification equivalence | CORRECTNESS | qualification | S | G-CORRECT |
| 58 | LIN-0523 | [D14](#d14) | Проверить projection pushdown equivalence | CORRECTNESS | qualification | S | G-CORRECT |
| 59 | LIN-0524 | [D14](#d14) | Проверить limit pushdown equivalence | CORRECTNESS | qualification | S | G-CORRECT |
| 60 | LIN-0722 | [D19](#d19) | Проверить NaN groups cardinality | CORRECTNESS | done | S | G-CORRECT |
| 61 | LIN-0723 | [D19](#d19) | Проверить signed-zero group identity | CORRECTNESS | done | S | G-CORRECT |
| 62 | LIN-0724 | [D19](#d19) | Проверить mixed int/float group identity | CORRECTNESS | done | S | G-CORRECT |
| 63 | LIN-0881 | [D23](#d23) | Профилировать FTS build tokenization | PERF | candidate | M | G-PERF |
| 64 | LIN-0882 | [D23](#d23) | Профилировать FTS build postings | PERF | candidate | M | G-PERF |
| 65 | LIN-0883 | [D23](#d23) | Профилировать lowercase scratch reuse | PERF | candidate | M | G-PERF |
| 66 | LIN-0884 | [D23](#d23) | Профилировать token deduplication | PERF | candidate | M | G-PERF |
| 67 | LIN-0961 | [D25](#d25) | Проверить token two-cursor diff | CORRECTNESS | done | S | G-CORRECT |
| 68 | LIN-0962 | [D25](#d25) | Проверить snippet-only update | CORRECTNESS | done | S | G-CORRECT |
| 69 | LIN-0963 | [D25](#d25) | Проверить removal последнего term | CORRECTNESS | done | S | G-CORRECT |
| 70 | LIN-0964 | [D25](#d25) | Проверить insertion нового term | CORRECTNESS | done | S | G-CORRECT |
| 71 | LIN-1041 | [D27](#d27) | Зафиксировать exact/approximate search contract | CORRECTNESS | qualification | M | G-CORRECT |
| 72 | LIN-1042 | [D27](#d27) | Измерить HNSW recall против exact | PERF | candidate | L | G-PERF |
| 73 | LIN-1043 | [D27](#d27) | Измерить HNSW build latency | PERF | candidate | L | G-PERF |
| 74 | LIN-1044 | [D27](#d27) | Измерить HNSW resident memory | PERF | candidate | L | G-PERF |
| 75 | LIN-1081 | [D28](#d28) | Проверить cosine dimensional mismatch | CORRECTNESS | qualification | S | G-CORRECT |
| 76 | LIN-1082 | [D28](#d28) | Проверить cosine zero norms | CORRECTNESS | qualification | S | G-CORRECT |
| 77 | LIN-1083 | [D28](#d28) | Проверить cosine NaN handling | CORRECTNESS | qualification | S | G-CORRECT |
| 78 | LIN-1084 | [D28](#d28) | Проверить cosine infinity handling | CORRECTNESS | qualification | S | G-CORRECT |
| 79 | LIN-1121 | [D29](#d29) | Проверить adapter selection diagnostics | CORRECTNESS | qualification | S | G-CORRECT |
| 80 | LIN-1122 | [D29](#d29) | Проверить device allocation failure | CORRECTNESS | qualification | S | G-CORRECT |
| 81 | LIN-1123 | [D29](#d29) | Проверить queue submission failure | CORRECTNESS | qualification | S | G-CORRECT |
| 82 | LIN-1124 | [D29](#d29) | Проверить device lost recovery | CORRECTNESS | qualification | S | G-CORRECT |
| 83 | LIN-1161 | [D30](#d30) | Проверить WGSL exact integer compares | CORRECTNESS | done | S | G-CORRECT |
| 84 | LIN-1162 | [D30](#d30) | Проверить WGSL mixed numeric compares | CORRECTNESS | done | S | G-CORRECT |
| 85 | LIN-1163 | [D30](#d30) | Проверить WGSL boolean combinations | CORRECTNESS | done | S | G-CORRECT |
| 86 | LIN-1164 | [D30](#d30) | Проверить WGSL null masks | CORRECTNESS | done | S | G-CORRECT |
| 87 | LIN-1321 | [D34](#d34) | Зафиксировать ReadDb visibility contract | CORRECTNESS | qualification | M | G-CORRECT |
| 88 | LIN-1322 | [D34](#d34) | Проверить snapshot generation isolation | CORRECTNESS | qualification | S | G-CORRECT |
| 89 | LIN-1323 | [D34](#d34) | Проверить snapshot index consistency | CORRECTNESS | qualification | S | G-CORRECT |
| 90 | LIN-1324 | [D34](#d34) | Проверить snapshot FTS consistency | CORRECTNESS | qualification | S | G-CORRECT |
| 91 | LIN-1361 | [D35](#d35) | Проверить handshake protocol version | CORRECTNESS | qualification | S | G-CORRECT |
| 92 | LIN-1362 | [D35](#d35) | Проверить server identity validation | CORRECTNESS | qualification | S | G-CORRECT |
| 93 | LIN-1363 | [D35](#d35) | Проверить transport framing limits | CORRECTNESS | qualification | S | G-CORRECT |
| 94 | LIN-1364 | [D35](#d35) | Проверить replication byte order | CORRECTNESS | qualification | S | G-CORRECT |
| 95 | LIN-1401 | [D36](#d36) | Проверить follower generation continuity | CORRECTNESS | qualification | S | G-CORRECT |
| 96 | LIN-1402 | [D36](#d36) | Проверить duplicate incoming records | CORRECTNESS | qualification | S | G-CORRECT |
| 97 | LIN-1403 | [D36](#d36) | Проверить out-of-order incoming records | CORRECTNESS | qualification | S | G-CORRECT |
| 98 | LIN-1404 | [D36](#d36) | Проверить missing incoming generation | CORRECTNESS | qualification | S | G-CORRECT |
| 99 | LIN-1641 | [D42](#d42) | Генерировать AST parser roundtrip cases | VERIFY | qualification | S | G-VERIFY |
| 100 | LIN-1642 | [D42](#d42) | Генерировать typed/untyped roundtrip cases | VERIFY | qualification | S | G-VERIFY |
| 101 | LIN-1643 | [D42](#d42) | Генерировать row/batch equivalence cases | VERIFY | qualification | S | G-VERIFY |
| 102 | LIN-1644 | [D42](#d42) | Генерировать cursor/materialized equivalence cases | VERIFY | qualification | S | G-VERIFY |
| 103 | LIN-1681 | [D43](#d43) | Инъецировать crash перед WAL append | VERIFY | qualification | S | G-VERIFY |
| 104 | LIN-1682 | [D43](#d43) | Инъецировать crash после header write | VERIFY | qualification | S | G-VERIFY |
| 105 | LIN-1683 | [D43](#d43) | Инъецировать crash после payload write | VERIFY | qualification | S | G-VERIFY |
| 106 | LIN-1684 | [D43](#d43) | Инъецировать crash после checksum write | VERIFY | qualification | S | G-VERIFY |
| 107 | LIN-1761 | [D45](#d45) | Ограничить untrusted query bytes | CORRECTNESS | qualification | M | G-CORRECT |
| 108 | LIN-1762 | [D45](#d45) | Ограничить prepared AST bytes | CORRECTNESS | qualification | M | G-CORRECT |
| 109 | LIN-1763 | [D45](#d45) | Ограничить query output bytes | CORRECTNESS | qualification | M | G-CORRECT |
| 110 | LIN-1764 | [D45](#d45) | Ограничить query execution work | CORRECTNESS | qualification | M | G-CORRECT |

### P2: 1290 пунктов

| Rank | ID | Область | Задача | Тип | Статус | Объём | Gate |
|---:|---|---|---|---|---|---|---|
| 111 | LIN-0009 | [D01](#d01) | Разделить CPU и RPC latency | PERF | candidate | M | G-PERF |
| 112 | LIN-0010 | [D01](#d01) | Разделить prepare и execute latency | PERF | candidate | M | G-PERF |
| 113 | LIN-0011 | [D01](#d01) | Уравнять подготовку DuckDB INSERT | PERF | candidate | M | G-PERF |
| 114 | LIN-0012 | [D01](#d01) | Уравнять создание входных DataFrame | PERF | candidate | M | G-PERF |
| 115 | LIN-0013 | [D01](#d01) | Зафиксировать actual engine versions | CORRECTNESS | qualification | M | G-CORRECT |
| 116 | LIN-0014 | [D01](#d01) | Сохранять runtime feature flags | PERF | candidate | M | G-PERF |
| 117 | LIN-0015 | [D01](#d01) | Проверять sample operation counts | PERF | candidate | M | G-PERF |
| 118 | LIN-0016 | [D01](#d01) | Измерять cold constructor latency | PERF | candidate | M | G-PERF |
| 119 | LIN-0017 | [D01](#d01) | Ввести matched mixed-case fixtures | VERIFY | qualification | S | G-VERIFY |
| 120 | LIN-0018 | [D01](#d01) | Проверять adversarial строковые fixtures | VERIFY | qualification | S | G-VERIFY |
| 121 | LIN-0019 | [D01](#d01) | Отдельно измерять tiny transactions | PERF | candidate | M | G-PERF |
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
| 141 | LIN-0045 | [D02](#d02) | Ограничить неоднозначность codec tags | CORRECTNESS | qualification | M | G-CORRECT |
| 142 | LIN-0046 | [D02](#d02) | Определить неизвестный future codec | CORRECTNESS | qualification | M | G-CORRECT |
| 143 | LIN-0047 | [D02](#d02) | Проверить endian независимость checksum | CORRECTNESS | qualification | S | G-CORRECT |
| 144 | LIN-0048 | [D02](#d02) | Проверить CRC пустого payload | CORRECTNESS | qualification | S | G-CORRECT |
| 145 | LIN-0049 | [D02](#d02) | Проверить CRC сегментированных update | CORRECTNESS | qualification | S | G-CORRECT |
| 146 | LIN-0050 | [D02](#d02) | Проверить CRC unaligned slices | CORRECTNESS | qualification | S | G-CORRECT |
| 147 | LIN-0051 | [D02](#d02) | Проверить CRC hardware fallback | CORRECTNESS | qualification | S | G-CORRECT |
| 148 | LIN-0052 | [D02](#d02) | Проверить CRC на длинном tail | CORRECTNESS | qualification | S | G-CORRECT |
| 149 | LIN-0053 | [D02](#d02) | Ограничить length arithmetic envelope | CORRECTNESS | qualification | M | G-CORRECT |
| 150 | LIN-0054 | [D02](#d02) | Проверить integer overflow record length | CORRECTNESS | qualification | S | G-CORRECT |
| 151 | LIN-0055 | [D02](#d02) | Проверить codec CRC coverage | CORRECTNESS | qualification | S | G-CORRECT |
| 152 | LIN-0056 | [D02](#d02) | Проверить checksum header mutations | CORRECTNESS | qualification | S | G-CORRECT |
| 153 | LIN-0057 | [D02](#d02) | Проверить sparse None против empty | CORRECTNESS | qualification | S | G-CORRECT |
| 154 | LIN-0058 | [D02](#d02) | Проверить сохранение negative zero | CORRECTNESS | qualification | S | G-CORRECT |
| 155 | LIN-0059 | [D02](#d02) | Проверить NaN payload при WAL | CORRECTNESS | qualification | S | G-CORRECT |
| 156 | LIN-0060 | [D02](#d02) | Проверить infinity payload при WAL | CORRECTNESS | qualification | S | G-CORRECT |
| 157 | LIN-0061 | [D02](#d02) | Уменьшить vector frame copies | CORRECTNESS | qualification | M | G-CORRECT |
| 158 | LIN-0062 | [D02](#d02) | Переиспользовать точный serialized size | CORRECTNESS | qualification | M | G-CORRECT |
| 159 | LIN-0063 | [D02](#d02) | Измерить segmented WAL encoding | PERF | candidate | M | G-PERF |
| 160 | LIN-0064 | [D02](#d02) | Измерить reserve buffer policy | PERF | candidate | M | G-PERF |
| 161 | LIN-0065 | [D02](#d02) | Разделить codec policy и encoder | CORRECTNESS | qualification | M | G-CORRECT |
| 162 | LIN-0066 | [D02](#d02) | Ввести явный decoder capability set | CORRECTNESS | qualification | M | G-CORRECT |
| 163 | LIN-0067 | [D02](#d02) | Документировать frame layout байтами | CORRECTNESS | qualification | M | G-CORRECT |
| 164 | LIN-0068 | [D02](#d02) | Подготовить независимый frame inspector | CORRECTNESS | qualification | M | G-CORRECT |
| 165 | LIN-0069 | [D02](#d02) | Проверить compressed envelope nesting | CORRECTNESS | qualification | S | G-CORRECT |
| 166 | LIN-0070 | [D02](#d02) | Ограничить decompression amplification | CORRECTNESS | qualification | M | G-CORRECT |
| 167 | LIN-0071 | [D02](#d02) | Проверить decoded vector budget | CORRECTNESS | qualification | S | G-CORRECT |
| 168 | LIN-0072 | [D02](#d02) | Проверить truncated coordinate pair | CORRECTNESS | qualification | S | G-CORRECT |
| 169 | LIN-0073 | [D02](#d02) | Проверить повторяющийся sparse index | CORRECTNESS | qualification | S | G-CORRECT |
| 170 | LIN-0074 | [D02](#d02) | Проверить unordered sparse coordinates | CORRECTNESS | qualification | S | G-CORRECT |
| 171 | LIN-0075 | [D02](#d02) | Проверить sparse dimension overflow | CORRECTNESS | qualification | S | G-CORRECT |
| 172 | LIN-0076 | [D02](#d02) | Проверить неверный nonzero count | CORRECTNESS | qualification | S | G-CORRECT |
| 173 | LIN-0077 | [D02](#d02) | Проверить одинаковые bytes разных CPU | CORRECTNESS | qualification | S | G-CORRECT |
| 174 | LIN-0078 | [D02](#d02) | Создать corpus исторических WAL frames | VERIFY | qualification | S | G-VERIFY |
| 175 | LIN-0079 | [D02](#d02) | Версионировать WAL golden fixtures | VERIFY | qualification | S | G-VERIFY |
| 176 | LIN-0080 | [D02](#d02) | Проверить codec migration rollback | CORRECTNESS | qualification | S | G-CORRECT |
| 177 | LIN-0085 | [D03](#d03) | Проверить barrier semantics Darwin | CORRECTNESS | qualification | S | G-CORRECT |
| 178 | LIN-0086 | [D03](#d03) | Проверить fdatasync semantics Linux | CORRECTNESS | qualification | S | G-CORRECT |
| 179 | LIN-0087 | [D03](#d03) | Проверить sync error propagation | CORRECTNESS | qualification | S | G-CORRECT |
| 180 | LIN-0088 | [D03](#d03) | Проверить interrupted sync retry | CORRECTNESS | qualification | S | G-CORRECT |
| 181 | LIN-0089 | [D03](#d03) | Сопоставить acknowledged durable boundaries | PERF | candidate | M | G-PERF |
| 182 | LIN-0090 | [D03](#d03) | Проверить writev short writes | CORRECTNESS | qualification | S | G-CORRECT |
| 183 | LIN-0091 | [D03](#d03) | Проверить writev Interrupted | CORRECTNESS | qualification | S | G-CORRECT |
| 184 | LIN-0092 | [D03](#d03) | Проверить writev WriteZero | CORRECTNESS | qualification | S | G-CORRECT |
| 185 | LIN-0093 | [D03](#d03) | Ограничить iovec batch длину | CORRECTNESS | qualification | M | G-CORRECT |
| 186 | LIN-0094 | [D03](#d03) | Проверить empty iovec handling | CORRECTNESS | qualification | S | G-CORRECT |
| 187 | LIN-0095 | [D03](#d03) | Измерить header coalescing | PERF | candidate | M | G-PERF |
| 188 | LIN-0096 | [D03](#d03) | Измерить reusable frame buffers | PERF | candidate | M | G-PERF |
| 189 | LIN-0097 | [D03](#d03) | Измерить WAL preallocation | PERF | candidate | M | G-PERF |
| 190 | LIN-0098 | [D03](#d03) | Измерить append file fragmentation | PERF | candidate | M | G-PERF |
| 191 | LIN-0099 | [D03](#d03) | Измерить buffered против direct I/O | PERF | candidate | M | G-PERF |
| 192 | LIN-0100 | [D03](#d03) | Измерить sequential write advice | PERF | candidate | M | G-PERF |
| 193 | LIN-0101 | [D03](#d03) | Ограничить рост retained encode buffer | CORRECTNESS | qualification | M | G-CORRECT |
| 194 | LIN-0102 | [D03](#d03) | Разделить throughput и sync latency | PERF | candidate | M | G-PERF |
| 195 | LIN-0103 | [D03](#d03) | Ввести per-commit sync telemetry | PERF | candidate | M | G-PERF |
| 196 | LIN-0104 | [D03](#d03) | Разделить encode/write/sync histograms | PERF | candidate | M | G-PERF |
| 197 | LIN-0105 | [D03](#d03) | Сопоставить групповой commit fairness | PERF | candidate | M | G-PERF |
| 198 | LIN-0106 | [D03](#d03) | Ограничить group commit wait | CORRECTNESS | qualification | M | G-CORRECT |
| 199 | LIN-0107 | [D03](#d03) | Ограничить group commit bytes | CORRECTNESS | qualification | M | G-CORRECT |
| 200 | LIN-0108 | [D03](#d03) | Проверить mixed sync policies | CORRECTNESS | qualification | S | G-CORRECT |
| 201 | LIN-0109 | [D03](#d03) | Проверить directory sync после rename | CORRECTNESS | qualification | S | G-CORRECT |
| 202 | LIN-0110 | [D03](#d03) | Проверить file creation durability | CORRECTNESS | qualification | S | G-CORRECT |
| 203 | LIN-0111 | [D03](#d03) | Проверить truncate durability ordering | CORRECTNESS | qualification | S | G-CORRECT |
| 204 | LIN-0112 | [D03](#d03) | Проверить WAL rotation ordering | CORRECTNESS | qualification | S | G-CORRECT |
| 205 | LIN-0113 | [D03](#d03) | Проверить out-of-space до sync | CORRECTNESS | qualification | S | G-CORRECT |
| 206 | LIN-0114 | [D03](#d03) | Проверить file descriptor exhaustion | CORRECTNESS | qualification | S | G-CORRECT |
| 207 | LIN-0115 | [D03](#d03) | Проверить read-only filesystem error | CORRECTNESS | qualification | S | G-CORRECT |
| 208 | LIN-0116 | [D03](#d03) | Проверить quota exhaustion propagation | CORRECTNESS | qualification | S | G-CORRECT |
| 209 | LIN-0117 | [D03](#d03) | Измерить tiny WAL commits | PERF | candidate | M | G-PERF |
| 210 | LIN-0118 | [D03](#d03) | Измерить large WAL commits | PERF | candidate | M | G-PERF |
| 211 | LIN-0119 | [D03](#d03) | Измерить concurrent-reader sync impact | PERF | candidate | M | G-PERF |
| 212 | LIN-0120 | [D03](#d03) | Измерить fsync на реальном диске | PERF | candidate | M | G-PERF |
| 213 | LIN-0125 | [D04](#d04) | Различать corruption и incomplete tail | CORRECTNESS | qualification | M | G-CORRECT |
| 214 | LIN-0126 | [D04](#d04) | Ограничить scanning повреждённого WAL | CORRECTNESS | qualification | L | G-CORRECT |
| 215 | LIN-0127 | [D04](#d04) | Проверить replay sequence monotonicity | CORRECTNESS | qualification | S | G-CORRECT |
| 216 | LIN-0128 | [D04](#d04) | Проверить duplicate record replay | CORRECTNESS | qualification | S | G-CORRECT |
| 217 | LIN-0129 | [D04](#d04) | Проверить generation discontinuity | CORRECTNESS | qualification | S | G-CORRECT |
| 218 | LIN-0130 | [D04](#d04) | Проверить next_id восстановление | CORRECTNESS | qualification | S | G-CORRECT |
| 219 | LIN-0131 | [D04](#d04) | Проверить deleted ID после replay | CORRECTNESS | qualification | S | G-CORRECT |
| 220 | LIN-0132 | [D04](#d04) | Проверить URI map после replay | CORRECTNESS | qualification | S | G-CORRECT |
| 221 | LIN-0133 | [D04](#d04) | Проверить scalar indexes после replay | CORRECTNESS | qualification | S | G-CORRECT |
| 222 | LIN-0134 | [D04](#d04) | Проверить FTS после replay | CORRECTNESS | qualification | S | G-CORRECT |
| 223 | LIN-0135 | [D04](#d04) | Проверить vector metadata после replay | CORRECTNESS | qualification | S | G-CORRECT |
| 224 | LIN-0136 | [D04](#d04) | Проверить graph edges после replay | CORRECTNESS | qualification | S | G-CORRECT |
| 225 | LIN-0137 | [D04](#d04) | Проверить replay без snapshot | CORRECTNESS | qualification | S | G-CORRECT |
| 226 | LIN-0138 | [D04](#d04) | Проверить snapshot без WAL | CORRECTNESS | qualification | S | G-CORRECT |
| 227 | LIN-0139 | [D04](#d04) | Проверить старый snapshot новый WAL | CORRECTNESS | qualification | S | G-CORRECT |
| 228 | LIN-0140 | [D04](#d04) | Проверить новый snapshot старый WAL | CORRECTNESS | qualification | S | G-CORRECT |
| 229 | LIN-0141 | [D04](#d04) | Проверить replay partial transaction | CORRECTNESS | qualification | S | G-CORRECT |
| 230 | LIN-0142 | [D04](#d04) | Проверить replay group boundaries | CORRECTNESS | qualification | S | G-CORRECT |
| 231 | LIN-0143 | [D04](#d04) | Проверить rollback после decode failure | CORRECTNESS | qualification | S | G-CORRECT |
| 232 | LIN-0144 | [D04](#d04) | Проверить read-only recovery path | CORRECTNESS | qualification | S | G-CORRECT |
| 233 | LIN-0145 | [D04](#d04) | Измерить bounded replay memory | PERF | candidate | M | G-PERF |
| 234 | LIN-0146 | [D04](#d04) | Измерить hot replay throughput | PERF | candidate | M | G-PERF |
| 235 | LIN-0147 | [D04](#d04) | Измерить cold replay throughput | PERF | candidate | M | G-PERF |
| 236 | LIN-0148 | [D04](#d04) | Профилировать metadata rebuild replay | PERF | candidate | M | G-PERF |
| 237 | LIN-0149 | [D04](#d04) | Добавить progress recovery callbacks | FEATURE | candidate | M | G-FEATURE |
| 238 | LIN-0150 | [D04](#d04) | Добавить cancellation recovery contract | FEATURE | candidate | M | G-FEATURE |
| 239 | LIN-0151 | [D04](#d04) | Добавить dry-run recovery report | FEATURE | candidate | M | G-FEATURE |
| 240 | LIN-0152 | [D04](#d04) | Добавить damaged-frame location diagnostics | FEATURE | candidate | M | G-FEATURE |
| 241 | LIN-0153 | [D04](#d04) | Проверить replay unknown collection | CORRECTNESS | qualification | S | G-CORRECT |
| 242 | LIN-0154 | [D04](#d04) | Проверить replay удалённого catalog object | CORRECTNESS | qualification | S | G-CORRECT |
| 243 | LIN-0155 | [D04](#d04) | Проверить replay несовместимого field type | CORRECTNESS | qualification | S | G-CORRECT |
| 244 | LIN-0156 | [D04](#d04) | Проверить replay schema generation mismatch | CORRECTNESS | qualification | S | G-CORRECT |
| 245 | LIN-0157 | [D04](#d04) | Проверить многократное reopen равенство | CORRECTNESS | qualification | S | G-CORRECT |
| 246 | LIN-0158 | [D04](#d04) | Проверить reopen после failed checkpoint | CORRECTNESS | qualification | S | G-CORRECT |
| 247 | LIN-0159 | [D04](#d04) | Проверить reopen после failed backup | CORRECTNESS | qualification | S | G-CORRECT |
| 248 | LIN-0160 | [D04](#d04) | Проверить reopen после truncated rotation | CORRECTNESS | qualification | S | G-CORRECT |
| 249 | LIN-0165 | [D05](#d05) | Проверить SnapshotRef lifetime | CORRECTNESS | qualification | S | G-CORRECT |
| 250 | LIN-0166 | [D05](#d05) | Проверить mutation во время snapshot | CORRECTNESS | qualification | S | G-CORRECT |
| 251 | LIN-0167 | [D05](#d05) | Проверить checkpoint generation fencing | CORRECTNESS | qualification | S | G-CORRECT |
| 252 | LIN-0168 | [D05](#d05) | Проверить snapshot checksum coverage | CORRECTNESS | qualification | S | G-CORRECT |
| 253 | LIN-0169 | [D05](#d05) | Ввести versioned snapshot manifest | PERF | candidate | M | G-PERF |
| 254 | LIN-0170 | [D05](#d05) | Проверить snapshot footer validation | CORRECTNESS | qualification | S | G-CORRECT |
| 255 | LIN-0171 | [D05](#d05) | Ограничить snapshot section sizes | CORRECTNESS | qualification | M | G-CORRECT |
| 256 | LIN-0172 | [D05](#d05) | Ограничить nested collection sizes | CORRECTNESS | qualification | M | G-CORRECT |
| 257 | LIN-0173 | [D05](#d05) | Измерить incremental checkpoint | PERF | candidate | M | G-PERF |
| 258 | LIN-0174 | [D05](#d05) | Измерить section-local compression | PERF | candidate | M | G-PERF |
| 259 | LIN-0175 | [D05](#d05) | Измерить parallel snapshot sections | PERF | candidate | M | G-PERF |
| 260 | LIN-0176 | [D05](#d05) | Измерить streaming snapshot encoder | PERF | done | M | G-PERF |
| 261 | LIN-0177 | [D05](#d05) | Проверить atomic snapshot replacement | CORRECTNESS | qualification | S | G-CORRECT |
| 262 | LIN-0178 | [D05](#d05) | Проверить failed rename cleanup | CORRECTNESS | qualification | S | G-CORRECT |
| 263 | LIN-0179 | [D05](#d05) | Проверить temp snapshot ownership | CORRECTNESS | qualification | S | G-CORRECT |
| 264 | LIN-0180 | [D05](#d05) | Проверить parent directory persistence | CORRECTNESS | qualification | S | G-CORRECT |
| 265 | LIN-0181 | [D05](#d05) | Проверить snapshot catalog consistency | CORRECTNESS | qualification | S | G-CORRECT |
| 266 | LIN-0182 | [D05](#d05) | Проверить snapshot edge consistency | CORRECTNESS | qualification | S | G-CORRECT |
| 267 | LIN-0183 | [D05](#d05) | Проверить snapshot derived indexes policy | CORRECTNESS | qualification | S | G-CORRECT |
| 268 | LIN-0184 | [D05](#d05) | Проверить snapshot embedding identity | CORRECTNESS | qualification | S | G-CORRECT |
| 269 | LIN-0205 | [D06](#d06) | Проверить rollback embeddings | CORRECTNESS | qualification | S | G-CORRECT |
| 270 | LIN-0206 | [D06](#d06) | Проверить rollback graph edges | CORRECTNESS | qualification | S | G-CORRECT |
| 271 | LIN-0207 | [D06](#d06) | Проверить rollback generated IDs | CORRECTNESS | qualification | S | G-CORRECT |
| 272 | LIN-0208 | [D06](#d06) | Проверить rollback catalog changes | CORRECTNESS | qualification | S | G-CORRECT |
| 273 | LIN-0209 | [D06](#d06) | Проверить mixed insert/update/delete rollback | CORRECTNESS | qualification | S | G-CORRECT |
| 274 | LIN-0210 | [D06](#d06) | Проверить group commit failure atomicity | CORRECTNESS | qualification | S | G-CORRECT |
| 275 | LIN-0211 | [D06](#d06) | Проверить commit после validation failure | CORRECTNESS | qualification | S | G-CORRECT |
| 276 | LIN-0212 | [D06](#d06) | Проверить rollback после allocator failure | CORRECTNESS | qualification | S | G-CORRECT |
| 277 | LIN-0213 | [D06](#d06) | Ограничить transaction journal bytes | CORRECTNESS | qualification | M | G-CORRECT |
| 278 | LIN-0214 | [D06](#d06) | Ограничить transaction mutation count | CORRECTNESS | qualification | M | G-CORRECT |
| 279 | LIN-0215 | [D06](#d06) | Измерить rollback work amplification | PERF | candidate | M | G-PERF |
| 280 | LIN-0216 | [D06](#d06) | Измерить journal clone elimination | PERF | candidate | M | G-PERF |
| 281 | LIN-0217 | [D06](#d06) | Подготовить explicit transaction API | CORRECTNESS | qualification | M | G-CORRECT |
| 282 | LIN-0218 | [D06](#d06) | Спроектировать bounded savepoints | FEATURE | candidate | L | G-FEATURE |
| 283 | LIN-0219 | [D06](#d06) | Спроектировать read-your-writes contract | FEATURE | candidate | L | G-FEATURE |
| 284 | LIN-0220 | [D06](#d06) | Спроектировать nested transaction rejection | FEATURE | candidate | L | G-FEATURE |
| 285 | LIN-0221 | [D06](#d06) | Проверить duplicate ID внутри batch | CORRECTNESS | qualification | S | G-CORRECT |
| 286 | LIN-0222 | [D06](#d06) | Проверить duplicate URI внутри batch | CORRECTNESS | qualification | S | G-CORRECT |
| 287 | LIN-0223 | [D06](#d06) | Проверить collision с existing ID | CORRECTNESS | qualification | S | G-CORRECT |
| 288 | LIN-0224 | [D06](#d06) | Проверить collision с existing URI | CORRECTNESS | qualification | S | G-CORRECT |
| 289 | LIN-0225 | [D06](#d06) | Проверить FK перед partial mutation | CORRECTNESS | qualification | S | G-CORRECT |
| 290 | LIN-0226 | [D06](#d06) | Проверить unique index before commit | CORRECTNESS | qualification | S | G-CORRECT |
| 291 | LIN-0227 | [D06](#d06) | Проверить deferred validation ordering | CORRECTNESS | qualification | S | G-CORRECT |
| 292 | LIN-0228 | [D06](#d06) | Проверить deterministic failure precedence | CORRECTNESS | qualification | S | G-CORRECT |
| 293 | LIN-0229 | [D06](#d06) | Проверить transaction cancellation semantics | CORRECTNESS | qualification | S | G-CORRECT |
| 294 | LIN-0230 | [D06](#d06) | Проверить panic unwind rollback | CORRECTNESS | qualification | S | G-CORRECT |
| 295 | LIN-0231 | [D06](#d06) | Проверить poisoned transaction reuse | CORRECTNESS | qualification | S | G-CORRECT |
| 296 | LIN-0232 | [D06](#d06) | Проверить failed commit reopen consistency | CORRECTNESS | qualification | S | G-CORRECT |
| 297 | LIN-0233 | [D06](#d06) | Проверить multi-collection transaction journal | CORRECTNESS | qualification | S | G-CORRECT |
| 298 | LIN-0234 | [D06](#d06) | Проверить transaction schema drift | CORRECTNESS | qualification | S | G-CORRECT |
| 299 | LIN-0235 | [D06](#d06) | Проверить concurrent snapshot visibility | CORRECTNESS | qualification | S | G-CORRECT |
| 300 | LIN-0236 | [D06](#d06) | Проверить transaction result lifetime | CORRECTNESS | qualification | S | G-CORRECT |
| 301 | LIN-0237 | [D06](#d06) | Добавить transaction phase diagnostics | FEATURE | candidate | M | G-FEATURE |
| 302 | LIN-0238 | [D06](#d06) | Добавить rollback reason counters | FEATURE | candidate | M | G-FEATURE |
| 303 | LIN-0239 | [D06](#d06) | Добавить commit generation receipts | FEATURE | candidate | M | G-FEATURE |
| 304 | LIN-0240 | [D06](#d06) | Добавить transaction replay identifiers | FEATURE | candidate | M | G-FEATURE |
| 305 | LIN-0245 | [D07](#d07) | Измерить borrowed identity sets сейчас | PERF | candidate | M | G-PERF |
| 306 | LIN-0246 | [D07](#d07) | Измерить defaults materialization batching | PERF | candidate | M | G-PERF |
| 307 | LIN-0247 | [D07](#d07) | Измерить body hash cost | PERF | candidate | M | G-PERF |
| 308 | LIN-0248 | [D07](#d07) | Измерить staged field validation | PERF | candidate | M | G-PERF |
| 309 | LIN-0249 | [D07](#d07) | Проверить borrowed key lifetime | CORRECTNESS | qualification | S | G-CORRECT |
| 310 | LIN-0250 | [D07](#d07) | Проверить inline-text key stability | CORRECTNESS | qualification | S | G-CORRECT |
| 311 | LIN-0251 | [D07](#d07) | Проверить mixed explicit/generated IDs | CORRECTNESS | qualification | S | G-CORRECT |
| 312 | LIN-0252 | [D07](#d07) | Проверить empty bulk operation | CORRECTNESS | qualification | S | G-CORRECT |
| 313 | LIN-0253 | [D07](#d07) | Проверить heterogeneous bulk columns | CORRECTNESS | qualification | S | G-CORRECT |
| 314 | LIN-0254 | [D07](#d07) | Проверить missing-field bulk defaults | CORRECTNESS | qualification | S | G-CORRECT |
| 315 | LIN-0255 | [D07](#d07) | Проверить explicit-null bulk semantics | CORRECTNESS | qualification | S | G-CORRECT |
| 316 | LIN-0256 | [D07](#d07) | Проверить bulk timestamp capture | CORRECTNESS | qualification | S | G-CORRECT |
| 317 | LIN-0257 | [D07](#d07) | Проверить result elision границу 128 | CORRECTNESS | qualification | S | G-CORRECT |
| 318 | LIN-0258 | [D07](#d07) | Проверить bulk done.n exactness | CORRECTNESS | qualification | S | G-CORRECT |
| 319 | LIN-0259 | [D07](#d07) | Проверить partial bulk error result | CORRECTNESS | qualification | S | G-CORRECT |
| 320 | LIN-0260 | [D07](#d07) | Проверить large bulk memory release | CORRECTNESS | qualification | S | G-CORRECT |
| 321 | LIN-0261 | [D07](#d07) | Измерить streaming bulk input | PERF | candidate | M | G-PERF |
| 322 | LIN-0262 | [D07](#d07) | Спроектировать bounded ingestion chunks | FEATURE | candidate | L | G-FEATURE |
| 323 | LIN-0263 | [D07](#d07) | Спроектировать ingestion receipts | FEATURE | candidate | L | G-FEATURE |
| 324 | LIN-0264 | [D07](#d07) | Спроектировать retry-safe bulk identifiers | FEATURE | candidate | L | G-FEATURE |
| 325 | LIN-0265 | [D07](#d07) | Измерить changed-field update masks | PERF | candidate | M | G-PERF |
| 326 | LIN-0266 | [D07](#d07) | Измерить no-op update fast path | PERF | candidate | M | G-PERF |
| 327 | LIN-0267 | [D07](#d07) | Измерить multirow delete compaction | PERF | candidate | M | G-PERF |
| 328 | LIN-0268 | [D07](#d07) | Измерить swap-remove map repair | PERF | candidate | M | G-PERF |
| 329 | LIN-0269 | [D07](#d07) | Проверить swap-delete duplicate positions | CORRECTNESS | qualification | S | G-CORRECT |
| 330 | LIN-0270 | [D07](#d07) | Проверить update changing URI | CORRECTNESS | qualification | S | G-CORRECT |
| 331 | LIN-0271 | [D07](#d07) | Проверить update changing ID | CORRECTNESS | qualification | S | G-CORRECT |
| 332 | LIN-0272 | [D07](#d07) | Проверить update preserving indexed values | CORRECTNESS | qualification | S | G-CORRECT |
| 333 | LIN-0273 | [D07](#d07) | Проверить mutation output ownership | CORRECTNESS | qualification | S | G-CORRECT |
| 334 | LIN-0274 | [D07](#d07) | Проверить mutation projection consistency | CORRECTNESS | qualification | S | G-CORRECT |
| 335 | LIN-0275 | [D07](#d07) | Проверить mutation returning old/new values | CORRECTNESS | qualification | S | G-CORRECT |
| 336 | LIN-0276 | [D07](#d07) | Проверить mutation counters overflow | CORRECTNESS | qualification | S | G-CORRECT |
| 337 | LIN-0277 | [D07](#d07) | Добавить ingestion byte budget | FEATURE | candidate | M | G-FEATURE |
| 338 | LIN-0278 | [D07](#d07) | Добавить bulk cancellation boundary | FEATURE | candidate | M | G-FEATURE |
| 339 | LIN-0279 | [D07](#d07) | Добавить per-collection ingestion limits | FEATURE | candidate | M | G-FEATURE |
| 340 | LIN-0280 | [D07](#d07) | Добавить ingest backpressure diagnostics | FEATURE | candidate | M | G-FEATURE |
| 341 | LIN-0285 | [D08](#d08) | Проверить Text/SmallText equality | CORRECTNESS | done | S | G-CORRECT |
| 342 | LIN-0286 | [D08](#d08) | Проверить Text/SmallText ordering | CORRECTNESS | done | S | G-CORRECT |
| 343 | LIN-0287 | [D08](#d08) | Проверить Text/SmallText hashing | CORRECTNESS | done | S | G-CORRECT |
| 344 | LIN-0288 | [D08](#d08) | Проверить Text/SmallText serialization | CORRECTNESS | done | S | G-CORRECT |
| 345 | LIN-0289 | [D08](#d08) | Проверить SSO exact capacity boundary | CORRECTNESS | done | S | G-CORRECT |
| 346 | LIN-0290 | [D08](#d08) | Проверить SSO UTF-8 boundary | CORRECTNESS | qualification | S | G-CORRECT |
| 347 | LIN-0291 | [D08](#d08) | Проверить SSO empty string | CORRECTNESS | qualification | S | G-CORRECT |
| 348 | LIN-0292 | [D08](#d08) | Проверить SSO embedded NUL | CORRECTNESS | qualification | S | G-CORRECT |
| 349 | LIN-0293 | [D08](#d08) | Измерить SSO field length distribution | PERF | candidate | M | G-PERF |
| 350 | LIN-0294 | [D08](#d08) | Измерить Cell size regression | PERF | candidate | M | G-PERF |
| 351 | LIN-0295 | [D08](#d08) | Измерить cloned inline strings | PERF | candidate | M | G-PERF |
| 352 | LIN-0296 | [D08](#d08) | Измерить shared large strings | PERF | candidate | M | G-PERF |
| 353 | LIN-0297 | [D08](#d08) | Проверить integer/float exact equality | CORRECTNESS | qualification | S | G-CORRECT |
| 354 | LIN-0298 | [D08](#d08) | Проверить i64 extrema comparisons | CORRECTNESS | qualification | S | G-CORRECT |
| 355 | LIN-0299 | [D08](#d08) | Проверить timestamp/value distinction | CORRECTNESS | qualification | S | G-CORRECT |
| 356 | LIN-0300 | [D08](#d08) | Проверить NULL equality policy | CORRECTNESS | qualification | S | G-CORRECT |
| 357 | LIN-0301 | [D08](#d08) | Проверить mixed numeric sorting | CORRECTNESS | qualification | S | G-CORRECT |
| 358 | LIN-0303 | [D08](#d08) | Проверить numeric cast overflow | CORRECTNESS | qualification | S | G-CORRECT |
| 359 | LIN-0304 | [D08](#d08) | Проверить scalar display roundtrip | CORRECTNESS | qualification | S | G-CORRECT |
| 360 | LIN-0305 | [D08](#d08) | Спроектировать explicit decimal Cell | FEATURE | candidate | L | G-FEATURE |
| 361 | LIN-0306 | [D08](#d08) | Спроектировать binary payload Cell | FEATURE | candidate | L | G-FEATURE |
| 362 | LIN-0307 | [D08](#d08) | Спроектировать duration Cell | FEATURE | candidate | L | G-FEATURE |
| 363 | LIN-0308 | [D08](#d08) | Спроектировать UUID scalar handling | FEATURE | candidate | L | G-FEATURE |
| 364 | LIN-0309 | [D08](#d08) | Спроектировать field-name interning boundary | FEATURE | candidate | L | G-FEATURE |
| 365 | LIN-0310 | [D08](#d08) | Спроектировать borrowed RowView | FEATURE | candidate | L | G-FEATURE |
| 366 | LIN-0311 | [D08](#d08) | Спроектировать stable typed field IDs | FEATURE | candidate | L | G-FEATURE |
| 367 | LIN-0312 | [D08](#d08) | Спроектировать row version metadata | FEATURE | candidate | L | G-FEATURE |
| 368 | LIN-0313 | [D08](#d08) | Проверить serde backward compatibility | CORRECTNESS | qualification | S | G-CORRECT |
| 369 | LIN-0314 | [D08](#d08) | Проверить serde unknown variants | CORRECTNESS | qualification | S | G-CORRECT |
| 370 | LIN-0315 | [D08](#d08) | Проверить serde text escaping | CORRECTNESS | qualification | S | G-CORRECT |
| 371 | LIN-0316 | [D08](#d08) | Проверить serde allocation limits | CORRECTNESS | qualification | S | G-CORRECT |
| 372 | LIN-0317 | [D08](#d08) | Измерить inline-key layout alternatives | PERF | candidate | M | G-PERF |
| 373 | LIN-0318 | [D08](#d08) | Измерить hot/cold row metadata | PERF | candidate | M | G-PERF |
| 374 | LIN-0319 | [D08](#d08) | Измерить compressed null metadata | PERF | candidate | M | G-PERF |
| 375 | LIN-0320 | [D08](#d08) | Измерить scalar access dispatch | PERF | candidate | M | G-PERF |
| 376 | LIN-0321 | [D09](#d09) | Измерить row pool hit rate | PERF | candidate | M | G-PERF |
| 377 | LIN-0322 | [D09](#d09) | Измерить row pool retained bytes | PERF | candidate | M | G-PERF |
| 378 | LIN-0323 | [D09](#d09) | Ограничить thread-local pool capacity | CORRECTNESS | qualification | M | G-CORRECT |
| 379 | LIN-0324 | [D09](#d09) | Ограничить individual pooled row size | CORRECTNESS | qualification | M | G-CORRECT |
| 380 | LIN-0325 | [D09](#d09) | Проверить pool clear semantics | CORRECTNESS | qualification | S | G-CORRECT |
| 381 | LIN-0326 | [D09](#d09) | Проверить nested query pool reuse | CORRECTNESS | qualification | S | G-CORRECT |
| 382 | LIN-0327 | [D09](#d09) | Проверить panic pool recovery | CORRECTNESS | qualification | S | G-CORRECT |
| 383 | LIN-0328 | [D09](#d09) | Проверить async thread migration pool | CORRECTNESS | qualification | S | G-CORRECT |
| 384 | LIN-0329 | [D09](#d09) | Проверить no stale fields reuse | CORRECTNESS | qualification | S | G-CORRECT |
| 385 | LIN-0330 | [D09](#d09) | Проверить no stale vectors reuse | CORRECTNESS | qualification | S | G-CORRECT |
| 386 | LIN-0331 | [D09](#d09) | Проверить no stale projection reuse | CORRECTNESS | qualification | S | G-CORRECT |
| 387 | LIN-0332 | [D09](#d09) | Проверить pooled row alias isolation | CORRECTNESS | qualification | S | G-CORRECT |
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
| 400 | LIN-0365 | [D10](#d10) | Проверить unchanged-key fast path | CORRECTNESS | qualification | S | G-CORRECT |
| 401 | LIN-0366 | [D10](#d10) | Проверить changed-key update path | CORRECTNESS | qualification | S | G-CORRECT |
| 402 | LIN-0368 | [D10](#d10) | Проверить remove missing position | CORRECTNESS | qualification | S | G-CORRECT |
| 403 | LIN-0369 | [D10](#d10) | Измерить short posting inline storage | PERF | candidate | M | G-PERF |
| 404 | LIN-0370 | [D10](#d10) | Измерить large posting chunking | PERF | candidate | M | G-PERF |
| 405 | LIN-0371 | [D10](#d10) | Измерить sorted bulk index build | PERF | candidate | M | G-PERF |
| 406 | LIN-0372 | [D10](#d10) | Измерить range posting coalescing | PERF | candidate | M | G-PERF |
| 407 | LIN-0373 | [D10](#d10) | Измерить borrowed index bounds | PERF | candidate | M | G-PERF |
| 408 | LIN-0374 | [D10](#d10) | Измерить bounds scratch lifetime | PERF | candidate | M | G-PERF |
| 409 | LIN-0375 | [D10](#d10) | Измерить seek_each allocation count | PERF | candidate | M | G-PERF |
| 410 | LIN-0376 | [D10](#d10) | Измерить extend_into buffer reuse | PERF | candidate | M | G-PERF |
| 411 | LIN-0377 | [D10](#d10) | Проверить lexicographic composite bounds | CORRECTNESS | qualification | S | G-CORRECT |
| 412 | LIN-0378 | [D10](#d10) | Проверить inclusive lower bounds | CORRECTNESS | qualification | S | G-CORRECT |
| 413 | LIN-0379 | [D10](#d10) | Проверить exclusive upper bounds | CORRECTNESS | qualification | S | G-CORRECT |
| 414 | LIN-0380 | [D10](#d10) | Проверить unbounded range endpoints | CORRECTNESS | qualification | S | G-CORRECT |
| 415 | LIN-0381 | [D10](#d10) | Проверить NULL composite keys | CORRECTNESS | qualification | S | G-CORRECT |
| 416 | LIN-0382 | [D10](#d10) | Проверить mixed text representations keys | CORRECTNESS | qualification | S | G-CORRECT |
| 417 | LIN-0383 | [D10](#d10) | Проверить duplicate nonunique postings | CORRECTNESS | qualification | S | G-CORRECT |
| 418 | LIN-0384 | [D10](#d10) | Проверить empty index definition | CORRECTNESS | qualification | S | G-CORRECT |
| 419 | LIN-0385 | [D10](#d10) | Спроектировать covering index payload | FEATURE | candidate | L | G-FEATURE |
| 420 | LIN-0386 | [D10](#d10) | Спроектировать partial index predicates | FEATURE | candidate | L | G-FEATURE |
| 421 | LIN-0387 | [D10](#d10) | Спроектировать expression index contract | FEATURE | candidate | L | G-FEATURE |
| 422 | LIN-0388 | [D10](#d10) | Спроектировать index visibility generations | FEATURE | candidate | L | G-FEATURE |
| 423 | LIN-0389 | [D10](#d10) | Проверить index rebuild rollback | CORRECTNESS | qualification | S | G-CORRECT |
| 424 | LIN-0390 | [D10](#d10) | Проверить index drop active cursor | CORRECTNESS | qualification | S | G-CORRECT |
| 425 | LIN-0391 | [D10](#d10) | Проверить index build cancellation | CORRECTNESS | qualification | S | G-CORRECT |
| 426 | LIN-0392 | [D10](#d10) | Проверить index schema mutation safety | CORRECTNESS | qualification | S | G-CORRECT |
| 427 | LIN-0393 | [D10](#d10) | Добавить index selectivity telemetry | FEATURE | candidate | M | G-FEATURE |
| 428 | LIN-0394 | [D10](#d10) | Добавить posting fanout statistics | FEATURE | candidate | M | G-FEATURE |
| 429 | LIN-0395 | [D10](#d10) | Добавить index rebuild progress | FEATURE | candidate | M | G-FEATURE |
| 430 | LIN-0396 | [D10](#d10) | Добавить index consistency inspector | FEATURE | candidate | M | G-FEATURE |
| 431 | LIN-0397 | [D10](#d10) | Сравнить dense reverse map crossover | PERF | candidate | M | G-PERF |
| 432 | LIN-0398 | [D10](#d10) | Сравнить adaptive posting representation | PERF | candidate | M | G-PERF |
| 433 | LIN-0399 | [D10](#d10) | Сравнить bitmap postings для dense keys | PERF | candidate | M | G-PERF |
| 434 | LIN-0400 | [D10](#d10) | Сравнить hash-only equality indexes | PERF | candidate | M | G-PERF |
| 435 | LIN-0405 | [D11](#d11) | Проверить NaN unique key policy | CORRECTNESS | qualification | S | G-CORRECT |
| 436 | LIN-0406 | [D11](#d11) | Проверить negative-zero unique policy | CORRECTNESS | qualification | S | G-CORRECT |
| 437 | LIN-0407 | [D11](#d11) | Проверить NULL unique key policy | CORRECTNESS | qualification | S | G-CORRECT |
| 438 | LIN-0408 | [D11](#d11) | Проверить empty-string unique key | CORRECTNESS | qualification | S | G-CORRECT |
| 439 | LIN-0409 | [D11](#d11) | Проверить Unicode normalization uniqueness | CORRECTNESS | qualification | S | G-CORRECT |
| 440 | LIN-0410 | [D11](#d11) | Проверить case-sensitive URI uniqueness | CORRECTNESS | qualification | S | G-CORRECT |
| 441 | LIN-0411 | [D11](#d11) | Проверить SSO/Arc duplicate collisions | CORRECTNESS | qualification | S | G-CORRECT |
| 442 | LIN-0412 | [D11](#d11) | Проверить composite type-tag collisions | CORRECTNESS | qualification | S | G-CORRECT |
| 443 | LIN-0413 | [D11](#d11) | Проверить duplicate index declaration | CORRECTNESS | qualification | S | G-CORRECT |
| 444 | LIN-0414 | [D11](#d11) | Проверить conflicting index definition | CORRECTNESS | qualification | S | G-CORRECT |
| 445 | LIN-0415 | [D11](#d11) | Проверить existing-data unique validation | CORRECTNESS | qualification | S | G-CORRECT |
| 446 | LIN-0416 | [D11](#d11) | Проверить uniqueness после swap remove | CORRECTNESS | qualification | S | G-CORRECT |
| 447 | LIN-0417 | [D11](#d11) | Проверить unique update self-match | CORRECTNESS | qualification | S | G-CORRECT |
| 448 | LIN-0418 | [D11](#d11) | Проверить unique batch cross-conflicts | CORRECTNESS | qualification | S | G-CORRECT |
| 449 | LIN-0419 | [D11](#d11) | Проверить unique rollback reverse map | CORRECTNESS | qualification | S | G-CORRECT |
| 450 | LIN-0420 | [D11](#d11) | Проверить unique constraint error location | CORRECTNESS | qualification | S | G-CORRECT |
| 451 | LIN-0421 | [D11](#d11) | Спроектировать named constraints | FEATURE | candidate | L | G-FEATURE |
| 452 | LIN-0422 | [D11](#d11) | Спроектировать configurable NULL distinctness | FEATURE | candidate | L | G-FEATURE |
| 453 | LIN-0423 | [D11](#d11) | Спроектировать CHECK predicates | FEATURE | candidate | L | G-FEATURE |
| 454 | LIN-0424 | [D11](#d11) | Спроектировать foreign-key delete actions | FEATURE | candidate | L | G-FEATURE |
| 455 | LIN-0441 | [D12](#d12) | Улучшить span для вложенных выражений | API | candidate | M | G-API |
| 456 | LIN-0442 | [D12](#d12) | Уточнить unexpected-token diagnostics | API | candidate | M | G-API |
| 457 | LIN-0443 | [D12](#d12) | Уточнить unterminated-string diagnostics | API | candidate | M | G-API |
| 458 | LIN-0444 | [D12](#d12) | Уточнить trailing-input diagnostics | API | candidate | M | G-API |
| 459 | LIN-0445 | [D12](#d12) | Ограничить parser recursion depth | CORRECTNESS | qualification | M | G-CORRECT |
| 460 | LIN-0446 | [D12](#d12) | Ограничить expression token count | CORRECTNESS | qualification | M | G-CORRECT |
| 461 | LIN-0447 | [D12](#d12) | Ограничить numeric literal length | CORRECTNESS | qualification | M | G-CORRECT |
| 462 | LIN-0448 | [D12](#d12) | Ограничить escaped string expansion | CORRECTNESS | qualification | M | G-CORRECT |
| 463 | LIN-0449 | [D12](#d12) | Проверить Unicode identifiers policy | CORRECTNESS | qualification | S | G-CORRECT |
| 464 | LIN-0450 | [D12](#d12) | Проверить whitespace lexical classes | CORRECTNESS | qualification | S | G-CORRECT |
| 465 | LIN-0451 | [D12](#d12) | Проверить embedded NUL lexer input | CORRECTNESS | qualification | S | G-CORRECT |
| 466 | LIN-0452 | [D12](#d12) | Проверить CRLF source spans | CORRECTNESS | qualification | S | G-CORRECT |
| 467 | LIN-0453 | [D12](#d12) | Проверить operator precedence matrix | CORRECTNESS | qualification | S | G-CORRECT |
| 468 | LIN-0454 | [D12](#d12) | Проверить associativity of chained operators | CORRECTNESS | qualification | S | G-CORRECT |
| 469 | LIN-0455 | [D12](#d12) | Проверить unary minus boundaries | CORRECTNESS | qualification | S | G-CORRECT |
| 470 | LIN-0456 | [D12](#d12) | Проверить parenthesis recovery policy | CORRECTNESS | qualification | S | G-CORRECT |
| 471 | LIN-0457 | [D12](#d12) | Спроектировать parameter placeholder syntax | API | candidate | L | G-API |
| 472 | LIN-0458 | [D12](#d12) | Спроектировать named parameter binding | API | candidate | L | G-API |
| 473 | LIN-0459 | [D12](#d12) | Спроектировать batch statement separators | API | candidate | L | G-API |
| 474 | LIN-0460 | [D12](#d12) | Спроектировать multiline query formatting | API | candidate | L | G-API |
| 475 | LIN-0461 | [D12](#d12) | Проверить quoted field names | CORRECTNESS | qualification | S | G-CORRECT |
| 476 | LIN-0462 | [D12](#d12) | Проверить reserved keyword escaping | CORRECTNESS | qualification | S | G-CORRECT |
| 477 | LIN-0463 | [D12](#d12) | Проверить nested projection grammar | CORRECTNESS | qualification | S | G-CORRECT |
| 478 | LIN-0464 | [D12](#d12) | Проверить empty list grammar | CORRECTNESS | qualification | S | G-CORRECT |
| 479 | LIN-0485 | [D13](#d13) | Проверить invalid field detection | CORRECTNESS | qualification | S | G-CORRECT |
| 480 | LIN-0486 | [D13](#d13) | Проверить invalid collection detection | CORRECTNESS | qualification | S | G-CORRECT |
| 481 | LIN-0487 | [D13](#d13) | Проверить incompatible operator types | CORRECTNESS | qualification | S | G-CORRECT |
| 482 | LIN-0488 | [D13](#d13) | Проверить invalid projection aliases | CORRECTNESS | qualification | S | G-CORRECT |
| 483 | LIN-0489 | [D13](#d13) | Проверить aggregate type inference | CORRECTNESS | qualification | S | G-CORRECT |
| 484 | LIN-0490 | [D13](#d13) | Проверить window result type inference | CORRECTNESS | qualification | S | G-CORRECT |
| 485 | LIN-0491 | [D13](#d13) | Проверить join output name collisions | CORRECTNESS | qualification | S | G-CORRECT |
| 486 | LIN-0492 | [D13](#d13) | Проверить nested expression typing | CORRECTNESS | qualification | S | G-CORRECT |
| 487 | LIN-0493 | [D13](#d13) | Спроектировать nullable schema fields | FEATURE | candidate | L | G-FEATURE |
| 488 | LIN-0494 | [D13](#d13) | Спроектировать union scalar types | FEATURE | candidate | L | G-FEATURE |
| 489 | LIN-0495 | [D13](#d13) | Спроектировать strict schema mode | FEATURE | candidate | L | G-FEATURE |
| 490 | LIN-0496 | [D13](#d13) | Спроектировать inferred schema reporting | FEATURE | candidate | L | G-FEATURE |
| 491 | LIN-0497 | [D13](#d13) | Проверить constant-fold overflow | CORRECTNESS | qualification | S | G-CORRECT |
| 492 | LIN-0498 | [D13](#d13) | Проверить constant-fold NaN semantics | CORRECTNESS | qualification | S | G-CORRECT |
| 493 | LIN-0499 | [D13](#d13) | Проверить short-circuit error visibility | CORRECTNESS | qualification | S | G-CORRECT |
| 494 | LIN-0500 | [D13](#d13) | Проверить evaluation order side effects | CORRECTNESS | qualification | S | G-CORRECT |
| 495 | LIN-0501 | [D13](#d13) | Проверить checked cast boundaries | CORRECTNESS | qualification | S | G-CORRECT |
| 496 | LIN-0502 | [D13](#d13) | Проверить date/time literal validation | CORRECTNESS | qualification | S | G-CORRECT |
| 497 | LIN-0503 | [D13](#d13) | Проверить regex literal validation | CORRECTNESS | qualification | S | G-CORRECT |
| 498 | LIN-0504 | [D13](#d13) | Проверить vector dimension validation | CORRECTNESS | qualification | S | G-CORRECT |
| 499 | LIN-0505 | [D13](#d13) | Добавить type error suggestions | FEATURE | candidate | M | G-FEATURE |
| 500 | LIN-0506 | [D13](#d13) | Добавить related source spans | FEATURE | candidate | M | G-FEATURE |
| 501 | LIN-0507 | [D13](#d13) | Добавить explain type annotations | FEATURE | candidate | L | G-FEATURE |
| 502 | LIN-0508 | [D13](#d13) | Добавить machine-readable check diagnostics | FEATURE | candidate | M | G-FEATURE |
| 503 | LIN-0509 | [D13](#d13) | Проверить prepared query schema generation | CORRECTNESS | qualification | S | G-CORRECT |
| 504 | LIN-0510 | [D13](#d13) | Проверить prepared query stale field IDs | CORRECTNESS | qualification | S | G-CORRECT |
| 505 | LIN-0511 | [D13](#d13) | Проверить prepared query missing index | CORRECTNESS | qualification | S | G-CORRECT |
| 506 | LIN-0512 | [D13](#d13) | Проверить prepared parameter type changes | CORRECTNESS | qualification | S | G-CORRECT |
| 507 | LIN-0513 | [D13](#d13) | Измерить repeated check caching | PERF | candidate | M | G-PERF |
| 508 | LIN-0514 | [D13](#d13) | Измерить catalog lookup interning | PERF | candidate | M | G-PERF |
| 509 | LIN-0515 | [D13](#d13) | Измерить type constraint sharing | PERF | candidate | M | G-PERF |
| 510 | LIN-0516 | [D13](#d13) | Измерить lightweight query validation | PERF | candidate | M | G-PERF |
| 511 | LIN-0517 | [D13](#d13) | Согласовать CPU/GPU comparison typing | CORRECTNESS | qualification | M | G-CORRECT |
| 512 | LIN-0518 | [D13](#d13) | Согласовать row/batch NULL results | CORRECTNESS | qualification | M | G-CORRECT |
| 513 | LIN-0519 | [D13](#d13) | Согласовать cursor/materialized type errors | CORRECTNESS | qualification | M | G-CORRECT |
| 514 | LIN-0520 | [D13](#d13) | Согласовать typed/untyped query errors | CORRECTNESS | qualification | M | G-CORRECT |
| 515 | LIN-0525 | [D14](#d14) | Ограничить DNF combinatorial growth | CORRECTNESS | qualification | M | G-CORRECT |
| 516 | LIN-0526 | [D14](#d14) | Спроектировать lazy DNF iterator | FEATURE | candidate | L | G-FEATURE |
| 517 | LIN-0527 | [D14](#d14) | Измерить conjunct flattening copies | PERF | candidate | M | G-PERF |
| 518 | LIN-0528 | [D14](#d14) | Измерить plan-node allocation | PERF | candidate | M | G-PERF |
| 519 | LIN-0529 | [D14](#d14) | Проверить index residual predicates | CORRECTNESS | qualification | S | G-CORRECT |
| 520 | LIN-0530 | [D14](#d14) | Проверить OR branch duplicate elimination | CORRECTNESS | qualification | S | G-CORRECT |
| 521 | LIN-0531 | [D14](#d14) | Проверить AND branch index intersection | CORRECTNESS | qualification | S | G-CORRECT |
| 522 | LIN-0532 | [D14](#d14) | Проверить empty predicate plan | CORRECTNESS | qualification | S | G-CORRECT |
| 523 | LIN-0533 | [D14](#d14) | Спроектировать logical rewrite registry | FEATURE | candidate | L | G-FEATURE |
| 524 | LIN-0534 | [D14](#d14) | Спроектировать rewrite preconditions | FEATURE | candidate | L | G-FEATURE |
| 525 | LIN-0535 | [D14](#d14) | Спроектировать rewrite trace output | FEATURE | candidate | L | G-FEATURE |
| 526 | LIN-0536 | [D14](#d14) | Спроектировать optimizer iteration budget | FEATURE | candidate | L | G-FEATURE |
| 527 | LIN-0537 | [D14](#d14) | Добавить common-subexpression sharing | FEATURE | candidate | M | G-FEATURE |
| 528 | LIN-0538 | [D14](#d14) | Добавить deterministic expression classification | FEATURE | candidate | M | G-FEATURE |
| 529 | LIN-0539 | [D14](#d14) | Добавить volatile expression barriers | FEATURE | candidate | M | G-FEATURE |
| 530 | LIN-0540 | [D14](#d14) | Добавить normalized predicate fingerprints | FEATURE | candidate | M | G-FEATURE |
| 531 | LIN-0541 | [D14](#d14) | Проверить join filter pushdown | CORRECTNESS | qualification | S | G-CORRECT |
| 532 | LIN-0542 | [D14](#d14) | Проверить post-join alias resolution | CORRECTNESS | qualification | S | G-CORRECT |
| 533 | LIN-0543 | [D14](#d14) | Проверить aggregate filter placement | CORRECTNESS | qualification | S | G-CORRECT |
| 534 | LIN-0544 | [D14](#d14) | Проверить window filter placement | CORRECTNESS | qualification | S | G-CORRECT |
| 535 | LIN-0545 | [D14](#d14) | Проверить sort/limit commutation rules | CORRECTNESS | qualification | S | G-CORRECT |
| 536 | LIN-0546 | [D14](#d14) | Проверить skip/take commutation rules | CORRECTNESS | qualification | S | G-CORRECT |
| 537 | LIN-0547 | [D14](#d14) | Проверить projection/filter commutation | CORRECTNESS | qualification | S | G-CORRECT |
| 538 | LIN-0548 | [D14](#d14) | Проверить graph/filter commutation | CORRECTNESS | qualification | S | G-CORRECT |
| 539 | LIN-0549 | [D14](#d14) | Добавить plan equivalence oracle | VERIFY | qualification | S | G-VERIFY |
| 540 | LIN-0550 | [D14](#d14) | Добавить no-optimization execution mode | FEATURE | candidate | M | G-FEATURE |
| 541 | LIN-0551 | [D14](#d14) | Добавить reproducible plan fingerprints | FEATURE | candidate | M | G-FEATURE |
| 542 | LIN-0552 | [D14](#d14) | Добавить optimizer debug counters | FEATURE | candidate | M | G-FEATURE |
| 543 | LIN-0553 | [D14](#d14) | Измерить prepared plan reuse | PERF | candidate | M | G-PERF |
| 544 | LIN-0554 | [D14](#d14) | Измерить plan cache key construction | PERF | candidate | M | G-PERF |
| 545 | LIN-0555 | [D14](#d14) | Ограничить retained plan-cache bytes | CORRECTNESS | qualification | M | G-CORRECT |
| 546 | LIN-0556 | [D14](#d14) | Проверить schema-aware plan invalidation | CORRECTNESS | qualification | S | G-CORRECT |
| 547 | LIN-0557 | [D14](#d14) | Спроектировать row/batch backend selection | FEATURE | candidate | L | G-FEATURE |
| 548 | LIN-0558 | [D14](#d14) | Спроектировать GPU eligibility explanation | FEATURE | candidate | L | G-FEATURE |
| 549 | LIN-0559 | [D14](#d14) | Спроектировать exact/approximate plan labels | FEATURE | candidate | L | G-FEATURE |
| 550 | LIN-0560 | [D14](#d14) | Спроектировать plan stability contract | FEATURE | candidate | L | G-FEATURE |
| 551 | LIN-0561 | [D15](#d15) | Добавить collection cardinality statistics | FEATURE | candidate | M | G-FEATURE |
| 552 | LIN-0562 | [D15](#d15) | Добавить distinct-key estimates | FEATURE | candidate | M | G-FEATURE |
| 553 | LIN-0563 | [D15](#d15) | Добавить NULL fraction statistics | FEATURE | candidate | M | G-FEATURE |
| 554 | LIN-0564 | [D15](#d15) | Добавить text-length histograms | FEATURE | candidate | M | G-FEATURE |
| 555 | LIN-0565 | [D15](#d15) | Добавить posting fanout histograms | FEATURE | candidate | M | G-FEATURE |
| 556 | LIN-0566 | [D15](#d15) | Добавить numeric range histograms | FEATURE | candidate | M | G-FEATURE |
| 557 | LIN-0567 | [D15](#d15) | Добавить composite prefix statistics | FEATURE | candidate | M | G-FEATURE |
| 558 | LIN-0568 | [D15](#d15) | Добавить vector dimension statistics | FEATURE | candidate | M | G-FEATURE |
| 559 | LIN-0569 | [D15](#d15) | Спроектировать explicit ANALYZE operation | FEATURE | candidate | L | G-FEATURE |
| 560 | LIN-0570 | [D15](#d15) | Спроектировать bounded statistics sampling | FEATURE | candidate | L | G-FEATURE |
| 561 | LIN-0571 | [D15](#d15) | Спроектировать incremental statistics updates | FEATURE | candidate | L | G-FEATURE |
| 562 | LIN-0572 | [D15](#d15) | Спроектировать stale statistics detection | FEATURE | candidate | L | G-FEATURE |
| 563 | LIN-0573 | [D15](#d15) | Оценивать index seek cardinality | PERF | candidate | M | G-PERF |
| 564 | LIN-0574 | [D15](#d15) | Оценивать residual-filter selectivity | PERF | candidate | M | G-PERF |
| 565 | LIN-0575 | [D15](#d15) | Оценивать substring candidate count | PERF | candidate | M | G-PERF |
| 566 | LIN-0576 | [D15](#d15) | Оценивать graph traversal expansion | PERF | candidate | M | G-PERF |
| 567 | LIN-0577 | [D15](#d15) | Оценивать join output cardinality | PERF | candidate | M | G-PERF |
| 568 | LIN-0578 | [D15](#d15) | Оценивать join build-side bytes | PERF | candidate | M | G-PERF |
| 569 | LIN-0579 | [D15](#d15) | Оценивать sort memory demand | PERF | candidate | M | G-PERF |
| 570 | LIN-0580 | [D15](#d15) | Оценивать aggregation group count | PERF | candidate | M | G-PERF |
| 571 | LIN-0581 | [D15](#d15) | Измерить estimator error distributions | PERF | candidate | M | G-PERF |
| 572 | LIN-0582 | [D15](#d15) | Проверить adversarial skew estimates | CORRECTNESS | qualification | S | G-CORRECT |
| 573 | LIN-0583 | [D15](#d15) | Проверить correlated predicate estimates | CORRECTNESS | qualification | S | G-CORRECT |
| 574 | LIN-0584 | [D15](#d15) | Проверить sparse collection estimates | CORRECTNESS | qualification | S | G-CORRECT |
| 575 | LIN-0601 | [D16](#d16) | Профилировать operator boundary copies | PERF | candidate | M | G-PERF |
| 576 | LIN-0602 | [D16](#d16) | Профилировать residual predicate evaluation | PERF | candidate | M | G-PERF |
| 577 | LIN-0603 | [D16](#d16) | Профилировать row materialization cost | PERF | candidate | M | G-PERF |
| 578 | LIN-0604 | [D16](#d16) | Профилировать projected field access | PERF | candidate | M | G-PERF |
| 579 | LIN-0605 | [D16](#d16) | Измерить fused filter/project | PERF | candidate | M | G-PERF |
| 580 | LIN-0606 | [D16](#d16) | Измерить fused count/filter | PERF | candidate | M | G-PERF |
| 581 | LIN-0607 | [D16](#d16) | Измерить reusable predicate scratch | PERF | candidate | M | G-PERF |
| 582 | LIN-0608 | [D16](#d16) | Измерить field-ID expression access | PERF | candidate | M | G-PERF |
| 583 | LIN-0609 | [D16](#d16) | Проверить limit early termination | CORRECTNESS | qualification | S | G-CORRECT |
| 584 | LIN-0610 | [D16](#d16) | Проверить filter side-effect ordering | CORRECTNESS | qualification | S | G-CORRECT |
| 585 | LIN-0611 | [D16](#d16) | Проверить NOW capture consistency | CORRECTNESS | qualification | S | G-CORRECT |
| 586 | LIN-0612 | [D16](#d16) | Проверить per-query timestamp reuse | CORRECTNESS | qualification | S | G-CORRECT |
| 587 | LIN-0613 | [D16](#d16) | Спроектировать physical operator interface | FEATURE | candidate | L | G-FEATURE |
| 588 | LIN-0614 | [D16](#d16) | Спроектировать explicit execution budgets | FEATURE | candidate | L | G-FEATURE |
| 589 | LIN-0615 | [D16](#d16) | Спроектировать operator cancellation checkpoints | FEATURE | candidate | L | G-FEATURE |
| 590 | LIN-0616 | [D16](#d16) | Спроектировать plan-level memory accounting | FEATURE | candidate | L | G-FEATURE |
| 591 | LIN-0617 | [D16](#d16) | Добавить row-count operator counters | FEATURE | candidate | M | G-FEATURE |
| 592 | LIN-0618 | [D16](#d16) | Добавить bytes-in/bytes-out counters | FEATURE | candidate | M | G-FEATURE |
| 593 | LIN-0619 | [D16](#d16) | Добавить materialization reason diagnostics | FEATURE | candidate | M | G-FEATURE |
| 594 | LIN-0620 | [D16](#d16) | Добавить backend transition traces | FEATURE | candidate | M | G-FEATURE |
| 595 | LIN-0621 | [D16](#d16) | Проверить error после partial result | CORRECTNESS | qualification | S | G-CORRECT |
| 596 | LIN-0622 | [D16](#d16) | Проверить nested execution reentrancy | CORRECTNESS | qualification | S | G-CORRECT |
| 597 | LIN-0623 | [D16](#d16) | Проверить row pool между operators | CORRECTNESS | qualification | S | G-CORRECT |
| 598 | LIN-0624 | [D16](#d16) | Проверить huge skip handling | CORRECTNESS | qualification | S | G-CORRECT |
| 599 | LIN-0625 | [D16](#d16) | Измерить chunked batch execution | PERF | candidate | M | G-PERF |
| 600 | LIN-0626 | [D16](#d16) | Измерить adaptive batch sizing | PERF | candidate | M | G-PERF |
| 601 | LIN-0627 | [D16](#d16) | Измерить branch-specialized scalar loops | PERF | candidate | M | G-PERF |
| 602 | LIN-0628 | [D16](#d16) | Измерить hot/cold operator separation | PERF | candidate | M | G-PERF |
| 603 | LIN-0629 | [D16](#d16) | Спроектировать spillable execution nodes | FEATURE | candidate | L | G-FEATURE |
| 604 | LIN-0630 | [D16](#d16) | Спроектировать bounded pipeline queues | FEATURE | candidate | L | G-FEATURE |
| 605 | LIN-0631 | [D16](#d16) | Спроектировать execution admission control | FEATURE | candidate | L | G-FEATURE |
| 606 | LIN-0632 | [D16](#d16) | Спроектировать fairness между queries | FEATURE | candidate | L | G-FEATURE |
| 607 | LIN-0633 | [D16](#d16) | Проверить CPU feature dispatch fallback | CORRECTNESS | qualification | S | G-CORRECT |
| 608 | LIN-0634 | [D16](#d16) | Проверить disabled backend explicit errors | CORRECTNESS | qualification | S | G-CORRECT |
| 609 | LIN-0635 | [D16](#d16) | Проверить cross-backend output order | CORRECTNESS | qualification | S | G-CORRECT |
| 610 | LIN-0636 | [D16](#d16) | Проверить expression exceptional values | CORRECTNESS | qualification | S | G-CORRECT |
| 611 | LIN-0637 | [D16](#d16) | Создать per-operator reference evaluator | PERF | candidate | M | G-PERF |
| 612 | LIN-0638 | [D16](#d16) | Создать execution differential corpus | VERIFY | qualification | S | G-VERIFY |
| 613 | LIN-0639 | [D16](#d16) | Создать operator resource stress suite | PERF | candidate | M | G-PERF |
| 614 | LIN-0640 | [D16](#d16) | Создать deterministic execution trace mode | PERF | candidate | M | G-PERF |
| 615 | LIN-0641 | [D17](#d17) | Проверить lazy cursor construction | CORRECTNESS | qualification | S | G-CORRECT |
| 616 | LIN-0642 | [D17](#d17) | Проверить first-row latency | CORRECTNESS | qualification | S | G-CORRECT |
| 617 | LIN-0643 | [D17](#d17) | Проверить take без полного scan | CORRECTNESS | qualification | S | G-CORRECT |
| 618 | LIN-0644 | [D17](#d17) | Проверить skip без лишней projection | CORRECTNESS | qualification | S | G-CORRECT |
| 619 | LIN-0645 | [D17](#d17) | Проверить next_projected field filtering | CORRECTNESS | qualification | S | G-CORRECT |
| 620 | LIN-0646 | [D17](#d17) | Проверить projected row pool reuse | CORRECTNESS | qualification | S | G-CORRECT |
| 621 | LIN-0647 | [D17](#d17) | Проверить cursor output lifetime | CORRECTNESS | qualification | S | G-CORRECT |
| 622 | LIN-0648 | [D17](#d17) | Проверить cursor exhausted reuse | CORRECTNESS | qualification | S | G-CORRECT |
| 623 | LIN-0649 | [D17](#d17) | Проверить dropped cursor cleanup | CORRECTNESS | qualification | S | G-CORRECT |
| 624 | LIN-0650 | [D17](#d17) | Проверить canceled cursor cleanup | CORRECTNESS | qualification | S | G-CORRECT |
| 625 | LIN-0651 | [D17](#d17) | Проверить cursor after mutation | CORRECTNESS | qualification | S | G-CORRECT |
| 626 | LIN-0652 | [D17](#d17) | Проверить cursor after catalog change | CORRECTNESS | qualification | S | G-CORRECT |
| 627 | LIN-0653 | [D17](#d17) | Спроектировать borrowed cursor items | API | candidate | L | G-API |
| 628 | LIN-0654 | [D17](#d17) | Спроектировать batched cursor fetch | API | candidate | L | G-API |
| 629 | LIN-0655 | [D17](#d17) | Спроектировать cursor size hints | API | candidate | L | G-API |
| 630 | LIN-0656 | [D17](#d17) | Спроектировать resumable cursor tokens | API | candidate | L | G-API |
| 631 | LIN-0657 | [D17](#d17) | Измерить seek cursor opening | PERF | candidate | M | G-PERF |
| 632 | LIN-0658 | [D17](#d17) | Измерить range cursor stepping | PERF | candidate | M | G-PERF |
| 633 | LIN-0659 | [D17](#d17) | Измерить join cursor buffering | PERF | candidate | M | G-PERF |
| 634 | LIN-0660 | [D17](#d17) | Измерить vector cursor buffering | PERF | candidate | M | G-PERF |
| 635 | LIN-0661 | [D17](#d17) | Спроектировать stable cursor ordering | API | candidate | L | G-API |
| 636 | LIN-0662 | [D17](#d17) | Спроектировать keyset pagination | API | candidate | L | G-API |
| 637 | LIN-0663 | [D17](#d17) | Спроектировать cursor snapshot pinning | API | candidate | L | G-API |
| 638 | LIN-0664 | [D17](#d17) | Спроектировать cursor generation errors | API | candidate | L | G-API |
| 639 | LIN-0681 | [D18](#d18) | Проверить column length invariants | CORRECTNESS | qualification | S | G-CORRECT |
| 640 | LIN-0682 | [D18](#d18) | Проверить validity bitmap semantics | CORRECTNESS | qualification | S | G-CORRECT |
| 641 | LIN-0683 | [D18](#d18) | Проверить scalar column type stability | CORRECTNESS | qualification | S | G-CORRECT |
| 642 | LIN-0684 | [D18](#d18) | Проверить vector column dimension policy | CORRECTNESS | qualification | S | G-CORRECT |
| 643 | LIN-0685 | [D18](#d18) | Измерить row-to-column packing | PERF | candidate | M | G-PERF |
| 644 | LIN-0686 | [D18](#d18) | Измерить column-to-row draining | PERF | candidate | M | G-PERF |
| 645 | LIN-0687 | [D18](#d18) | Измерить projected column materialization | PERF | candidate | M | G-PERF |
| 646 | LIN-0688 | [D18](#d18) | Измерить batch ownership transfer | PERF | candidate | M | G-PERF |
| 647 | LIN-0689 | [D18](#d18) | Спроектировать borrowed ColumnView | FEATURE | candidate | L | G-FEATURE |
| 648 | LIN-0690 | [D18](#d18) | Спроектировать dictionary text columns | FEATURE | candidate | L | G-FEATURE |
| 649 | LIN-0691 | [D18](#d18) | Спроектировать offset-based binary columns | FEATURE | candidate | L | G-FEATURE |
| 650 | LIN-0692 | [D18](#d18) | Спроектировать dense numeric columns | FEATURE | candidate | L | G-FEATURE |
| 651 | LIN-0693 | [D18](#d18) | Проверить heterogeneous column fallback | CORRECTNESS | qualification | S | G-CORRECT |
| 652 | LIN-0694 | [D18](#d18) | Проверить all-null column type | CORRECTNESS | qualification | S | G-CORRECT |
| 653 | LIN-0695 | [D18](#d18) | Проверить empty batch schema | CORRECTNESS | qualification | S | G-CORRECT |
| 654 | LIN-0696 | [D18](#d18) | Проверить repeated field names | CORRECTNESS | qualification | S | G-CORRECT |
| 655 | LIN-0697 | [D18](#d18) | Добавить zero-copy batch slicing | FEATURE | candidate | M | G-FEATURE |
| 656 | LIN-0698 | [D18](#d18) | Добавить shared bitmap slicing | FEATURE | candidate | M | G-FEATURE |
| 657 | LIN-0699 | [D18](#d18) | Добавить batch concatenation contract | FEATURE | candidate | M | G-FEATURE |
| 658 | LIN-0700 | [D18](#d18) | Добавить batch schema reconciliation | FEATURE | candidate | M | G-FEATURE |
| 659 | LIN-0701 | [D18](#d18) | Измерить SIMD numeric predicates | PERF | candidate | M | G-PERF |
| 660 | LIN-0702 | [D18](#d18) | Измерить SIMD bitmap operations | PERF | candidate | M | G-PERF |
| 661 | LIN-0703 | [D18](#d18) | Измерить packed UTF-8 projection | PERF | candidate | M | G-PERF |
| 662 | LIN-0704 | [D18](#d18) | Измерить vector column alignment | PERF | candidate | M | G-PERF |
| 663 | LIN-0725 | [D19](#d19) | Проверить SSO/Arc group identity | CORRECTNESS | done | S | G-CORRECT |
| 664 | LIN-0726 | [D19](#d19) | Проверить NULL group identity | CORRECTNESS | done | S | G-CORRECT |
| 665 | LIN-0727 | [D19](#d19) | Проверить deterministic group order | CORRECTNESS | done | S | G-CORRECT |
| 666 | LIN-0728 | [D19](#d19) | Проверить empty-input aggregate results | CORRECTNESS | qualification | S | G-CORRECT |
| 667 | LIN-0729 | [D19](#d19) | Проверить count/count-field distinction | CORRECTNESS | qualification | S | G-CORRECT |
| 668 | LIN-0730 | [D19](#d19) | Проверить sum integer overflow | CORRECTNESS | qualification | S | G-CORRECT |
| 669 | LIN-0731 | [D19](#d19) | Проверить sum float order policy | CORRECTNESS | qualification | S | G-CORRECT |
| 670 | LIN-0732 | [D19](#d19) | Проверить avg precision policy | CORRECTNESS | qualification | S | G-CORRECT |
| 671 | LIN-0733 | [D19](#d19) | Проверить min/max mixed scalar types | CORRECTNESS | qualification | S | G-CORRECT |
| 672 | LIN-0734 | [D19](#d19) | Проверить first/last source ordering | CORRECTNESS | qualification | S | G-CORRECT |
| 673 | LIN-0735 | [D19](#d19) | Проверить distinct value semantics | CORRECTNESS | qualification | S | G-CORRECT |
| 674 | LIN-0736 | [D19](#d19) | Проверить composite group keys | CORRECTNESS | qualification | S | G-CORRECT |
| 675 | LIN-0737 | [D19](#d19) | Измерить FxHashMap grouping capacity | PERF | candidate | M | G-PERF |
| 676 | LIN-0738 | [D19](#d19) | Измерить inline group key buffers | PERF | candidate | M | G-PERF |
| 677 | LIN-0739 | [D19](#d19) | Измерить group aggregate state layout | PERF | candidate | M | G-PERF |
| 678 | LIN-0740 | [D19](#d19) | Измерить grouped projection allocation | PERF | candidate | M | G-PERF |
| 679 | LIN-0741 | [D19](#d19) | Спроектировать streaming aggregation | FEATURE | candidate | L | G-FEATURE |
| 680 | LIN-0742 | [D19](#d19) | Спроектировать spillable hash aggregation | FEATURE | candidate | L | G-FEATURE |
| 681 | LIN-0743 | [D19](#d19) | Спроектировать sorted-key aggregation | FEATURE | candidate | L | G-FEATURE |
| 682 | LIN-0744 | [D19](#d19) | Спроектировать partial aggregate merge | FEATURE | candidate | L | G-FEATURE |
| 683 | LIN-0745 | [D19](#d19) | Добавить variance aggregate contract | FEATURE | candidate | M | G-FEATURE |
| 684 | LIN-0746 | [D19](#d19) | Добавить standard-deviation aggregate contract | FEATURE | candidate | M | G-FEATURE |
| 685 | LIN-0747 | [D19](#d19) | Добавить exact quantile aggregate | FEATURE | candidate | M | G-FEATURE |
| 686 | LIN-0748 | [D19](#d19) | Добавить approximate quantile opt-in | FEATURE | candidate | M | G-FEATURE |
| 687 | LIN-0749 | [D19](#d19) | Проверить aggregate FILTER clauses | CORRECTNESS | qualification | S | G-CORRECT |
| 688 | LIN-0750 | [D19](#d19) | Проверить aggregate alias visibility | CORRECTNESS | qualification | S | G-CORRECT |
| 689 | LIN-0751 | [D19](#d19) | Проверить HAVING-like stage semantics | CORRECTNESS | qualification | S | G-CORRECT |
| 690 | LIN-0752 | [D19](#d19) | Проверить nested aggregate rejection | CORRECTNESS | qualification | S | G-CORRECT |
| 691 | LIN-0753 | [D19](#d19) | Согласовать row/batch aggregate bits | CORRECTNESS | qualification | M | G-CORRECT |
| 692 | LIN-0754 | [D19](#d19) | Согласовать CPU/GPU aggregate results | CORRECTNESS | done | M | G-CORRECT |
| 693 | LIN-0755 | [D19](#d19) | Проверить large-integer GPU grouping | CORRECTNESS | qualification | S | G-CORRECT |
| 694 | LIN-0756 | [D19](#d19) | Проверить catastrophic float cancellation | CORRECTNESS | qualification | S | G-CORRECT |
| 695 | LIN-0757 | [D19](#d19) | Добавить group cardinality limits | FEATURE | candidate | M | G-FEATURE |
| 696 | LIN-0758 | [D19](#d19) | Добавить aggregate-state memory budget | FEATURE | candidate | M | G-FEATURE |
| 697 | LIN-0759 | [D19](#d19) | Добавить aggregation skew diagnostics | FEATURE | candidate | M | G-FEATURE |
| 698 | LIN-0760 | [D19](#d19) | Добавить group overflow diagnostics | FEATURE | candidate | M | G-FEATURE |
| 699 | LIN-0761 | [D20](#d20) | Проверить stable equal-key sorting | CORRECTNESS | qualification | S | G-CORRECT |
| 700 | LIN-0762 | [D20](#d20) | Проверить NULL placement policy | CORRECTNESS | qualification | S | G-CORRECT |
| 701 | LIN-0763 | [D20](#d20) | Проверить NaN placement policy | CORRECTNESS | qualification | S | G-CORRECT |
| 702 | LIN-0764 | [D20](#d20) | Проверить exact integer rank | CORRECTNESS | qualification | S | G-CORRECT |
| 703 | LIN-0765 | [D20](#d20) | Измерить decorated sort key reuse | PERF | candidate | M | G-PERF |
| 704 | LIN-0766 | [D20](#d20) | Измерить bounded Top-K crossover | PERF | candidate | M | G-PERF |
| 705 | LIN-0767 | [D20](#d20) | Измерить partial selection partition | PERF | candidate | M | G-PERF |
| 706 | LIN-0768 | [D20](#d20) | Измерить multi-key comparator cost | PERF | candidate | M | G-PERF |
| 707 | LIN-0769 | [D20](#d20) | Проверить mixed-type sort transitivity | CORRECTNESS | qualification | S | G-CORRECT |
| 708 | LIN-0770 | [D20](#d20) | Проверить reverse order tie stability | CORRECTNESS | qualification | S | G-CORRECT |
| 709 | LIN-0771 | [D20](#d20) | Проверить empty sort output | CORRECTNESS | qualification | S | G-CORRECT |
| 710 | LIN-0772 | [D20](#d20) | Проверить sort take zero | CORRECTNESS | qualification | S | G-CORRECT |
| 711 | LIN-0773 | [D20](#d20) | Проверить skip-plus-take overflow | CORRECTNESS | qualification | S | G-CORRECT |
| 712 | LIN-0774 | [D20](#d20) | Проверить Top-K identity ties | CORRECTNESS | qualification | S | G-CORRECT |
| 713 | LIN-0775 | [D20](#d20) | Проверить descending composite bounds | CORRECTNESS | qualification | S | G-CORRECT |
| 714 | LIN-0776 | [D20](#d20) | Проверить post-projection sort keys | CORRECTNESS | qualification | S | G-CORRECT |
| 715 | LIN-0777 | [D20](#d20) | Спроектировать spillable external sort | FEATURE | candidate | L | G-FEATURE |
| 716 | LIN-0778 | [D20](#d20) | Спроектировать merge sorted runs | FEATURE | candidate | L | G-FEATURE |
| 717 | LIN-0779 | [D20](#d20) | Спроектировать sorted-stream merge | FEATURE | candidate | L | G-FEATURE |
| 718 | LIN-0780 | [D20](#d20) | Спроектировать sort cancellation cleanup | FEATURE | candidate | L | G-FEATURE |
| 719 | LIN-0781 | [D20](#d20) | Добавить collation explicit policy | FEATURE | candidate | M | G-FEATURE |
| 720 | LIN-0782 | [D20](#d20) | Добавить locale collation opt-in | FEATURE | candidate | M | G-FEATURE |
| 721 | LIN-0783 | [D20](#d20) | Добавить binary collation baseline | FEATURE | candidate | M | G-FEATURE |
| 722 | LIN-0784 | [D20](#d20) | Добавить natural-sort opt-in | FEATURE | candidate | M | G-FEATURE |
| 723 | LIN-0801 | [D21](#d21) | Проверить duplicate join multiplicity | CORRECTNESS | qualification | S | G-CORRECT |
| 724 | LIN-0802 | [D21](#d21) | Проверить missing-key join behavior | CORRECTNESS | qualification | S | G-CORRECT |
| 725 | LIN-0803 | [D21](#d21) | Проверить NULL join keys | CORRECTNESS | qualification | S | G-CORRECT |
| 726 | LIN-0804 | [D21](#d21) | Проверить numeric cross-type join keys | CORRECTNESS | qualification | S | G-CORRECT |
| 727 | LIN-0805 | [D21](#d21) | Проверить asymmetric build-side selection | CORRECTNESS | qualification | S | G-CORRECT |
| 728 | LIN-0806 | [D21](#d21) | Профилировать hash table build cost | PERF | candidate | M | G-PERF |
| 729 | LIN-0807 | [D21](#d21) | Профилировать join result materialization | PERF | candidate | M | G-PERF |
| 730 | LIN-0808 | [D21](#d21) | Профилировать semi-join prefilter | PERF | candidate | M | G-PERF |
| 731 | LIN-0809 | [D21](#d21) | Измерить key-only join build | PERF | candidate | M | G-PERF |
| 732 | LIN-0810 | [D21](#d21) | Измерить borrowed join keys | PERF | candidate | M | G-PERF |
| 733 | LIN-0811 | [D21](#d21) | Измерить compact join payloads | PERF | candidate | M | G-PERF |
| 734 | LIN-0812 | [D21](#d21) | Измерить dense ID joins | PERF | candidate | M | G-PERF |
| 735 | LIN-0813 | [D21](#d21) | Спроектировать merge join operator | FEATURE | candidate | L | G-FEATURE |
| 736 | LIN-0814 | [D21](#d21) | Спроектировать left outer join | FEATURE | candidate | L | G-FEATURE |
| 737 | LIN-0815 | [D21](#d21) | Спроектировать anti-join semantics | FEATURE | candidate | L | G-FEATURE |
| 738 | LIN-0816 | [D21](#d21) | Спроектировать existence-only joins | FEATURE | candidate | L | G-FEATURE |
| 739 | LIN-0817 | [D21](#d21) | Проверить join output ordering | CORRECTNESS | qualification | S | G-CORRECT |
| 740 | LIN-0818 | [D21](#d21) | Проверить join field collisions | CORRECTNESS | qualification | S | G-CORRECT |
| 741 | LIN-0819 | [D21](#d21) | Проверить alias resolution across joins | CORRECTNESS | qualification | S | G-CORRECT |
| 742 | LIN-0820 | [D21](#d21) | Проверить multi-join associative rewrites | CORRECTNESS | qualification | S | G-CORRECT |
| 743 | LIN-0821 | [D21](#d21) | Добавить join output cardinality limit | FEATURE | candidate | M | G-FEATURE |
| 744 | LIN-0822 | [D21](#d21) | Добавить join memory budget | FEATURE | candidate | M | G-FEATURE |
| 745 | LIN-0823 | [D21](#d21) | Добавить spillable hash partitions | FEATURE | candidate | L | G-FEATURE |
| 746 | LIN-0824 | [D21](#d21) | Добавить join skew handling | FEATURE | candidate | M | G-FEATURE |
| 747 | LIN-0841 | [D22](#d22) | Проверить partition key hashing | CORRECTNESS | done | S | G-CORRECT |
| 748 | LIN-0842 | [D22](#d22) | Проверить NULL partition keys | CORRECTNESS | done | S | G-CORRECT |
| 749 | LIN-0843 | [D22](#d22) | Проверить numeric partition equivalence | CORRECTNESS | done | S | G-CORRECT |
| 750 | LIN-0844 | [D22](#d22) | Проверить SSO partition equivalence | CORRECTNESS | done | S | G-CORRECT |
| 751 | LIN-0845 | [D22](#d22) | Проверить source-order window stability | CORRECTNESS | qualification | S | G-CORRECT |
| 752 | LIN-0846 | [D22](#d22) | Проверить equal-order-key ties | CORRECTNESS | qualification | S | G-CORRECT |
| 753 | LIN-0847 | [D22](#d22) | Проверить ascending/descending frame bounds | CORRECTNESS | qualification | S | G-CORRECT |
| 754 | LIN-0848 | [D22](#d22) | Проверить empty frame outputs | CORRECTNESS | qualification | S | G-CORRECT |
| 755 | LIN-0849 | [D22](#d22) | Проверить prefix-sum frame subtraction | CORRECTNESS | qualification | S | G-CORRECT |
| 756 | LIN-0850 | [D22](#d22) | Проверить large integer window sums | CORRECTNESS | qualification | S | G-CORRECT |
| 757 | LIN-0851 | [D22](#d22) | Проверить float prefix cancellation | CORRECTNESS | qualification | S | G-CORRECT |
| 758 | LIN-0852 | [D22](#d22) | Проверить negative-zero window sums | CORRECTNESS | qualification | S | G-CORRECT |
| 759 | LIN-0853 | [D22](#d22) | Спроектировать ROWS frame syntax | FEATURE | candidate | L | G-FEATURE |
| 760 | LIN-0854 | [D22](#d22) | Спроектировать RANGE frame semantics | FEATURE | candidate | L | G-FEATURE |
| 761 | LIN-0855 | [D22](#d22) | Спроектировать GROUPS frame semantics | FEATURE | candidate | L | G-FEATURE |
| 762 | LIN-0856 | [D22](#d22) | Спроектировать excluded-current-row frame | FEATURE | candidate | L | G-FEATURE |
| 763 | LIN-0857 | [D22](#d22) | Проверить row_number boundaries | CORRECTNESS | qualification | S | G-CORRECT |
| 764 | LIN-0858 | [D22](#d22) | Проверить rank tie cases | VERIFY | qualification | S | G-VERIFY |
| 765 | LIN-0859 | [D22](#d22) | Проверить dense_rank tie cases | VERIFY | qualification | S | G-VERIFY |
| 766 | LIN-0860 | [D22](#d22) | Проверить percent_rank empty cases | VERIFY | qualification | S | G-VERIFY |
| 767 | LIN-0861 | [D22](#d22) | Проверить lag default semantics | CORRECTNESS | qualification | S | G-CORRECT |
| 768 | LIN-0862 | [D22](#d22) | Проверить lead default semantics | CORRECTNESS | qualification | S | G-CORRECT |
| 769 | LIN-0863 | [D22](#d22) | Проверить first_value frame semantics | CORRECTNESS | qualification | S | G-CORRECT |
| 770 | LIN-0864 | [D22](#d22) | Проверить last_value frame semantics | CORRECTNESS | qualification | S | G-CORRECT |
| 771 | LIN-0885 | [D23](#d23) | Проверить declared FTS fields union | CORRECTNESS | qualification | S | G-CORRECT |
| 772 | LIN-0887 | [D23](#d23) | Проверить empty token normalization | CORRECTNESS | qualification | S | G-CORRECT |
| 773 | LIN-0888 | [D23](#d23) | Проверить repeated token uniqueness | CORRECTNESS | qualification | S | G-CORRECT |
| 774 | LIN-0889 | [D23](#d23) | Проверить Greek contextual lowercase | CORRECTNESS | qualification | S | G-CORRECT |
| 775 | LIN-0890 | [D23](#d23) | Проверить Unicode whitespace classes | CORRECTNESS | qualification | S | G-CORRECT |
| 776 | LIN-0891 | [D23](#d23) | Проверить vertical-tab token boundaries | CORRECTNESS | qualification | S | G-CORRECT |
| 777 | LIN-0892 | [D23](#d23) | Проверить combining-mark token boundaries | CORRECTNESS | qualification | S | G-CORRECT |
| 778 | LIN-0893 | [D23](#d23) | Измерить ASCII token fast path | PERF | candidate | M | G-PERF |
| 779 | LIN-0894 | [D23](#d23) | Измерить byte-scan tokenizer crossover | PERF | candidate | M | G-PERF |
| 780 | LIN-0895 | [D23](#d23) | Измерить borrowed token lifetimes | PERF | candidate | M | G-PERF |
| 781 | LIN-0896 | [D23](#d23) | Измерить interned term dictionaries | PERF | candidate | M | G-PERF |
| 782 | LIN-0897 | [D23](#d23) | Спроектировать tokenizer identity version | FEATURE | candidate | L | G-FEATURE |
| 783 | LIN-0898 | [D23](#d23) | Спроектировать configurable token length | FEATURE | candidate | L | G-FEATURE |
| 784 | LIN-0899 | [D23](#d23) | Спроектировать stopword opt-in | FEATURE | candidate | L | G-FEATURE |
| 785 | LIN-0900 | [D23](#d23) | Спроектировать stemming opt-in | FEATURE | candidate | L | G-FEATURE |
| 786 | LIN-0901 | [D23](#d23) | Добавить token position storage | FEATURE | candidate | M | G-FEATURE |
| 787 | LIN-0902 | [D23](#d23) | Добавить field-local token metadata | FEATURE | candidate | M | G-FEATURE |
| 788 | LIN-0903 | [D23](#d23) | Добавить document length metadata | FEATURE | candidate | M | G-FEATURE |
| 789 | LIN-0904 | [D23](#d23) | Добавить original offset mapping | FEATURE | candidate | M | G-FEATURE |
| 790 | LIN-0905 | [D23](#d23) | Измерить parallel FTS build | PERF | candidate | M | G-PERF |
| 791 | LIN-0906 | [D23](#d23) | Измерить sorted term bulk build | PERF | candidate | M | G-PERF |
| 792 | LIN-0907 | [D23](#d23) | Измерить posting delta compression | PERF | candidate | M | G-PERF |
| 793 | LIN-0908 | [D23](#d23) | Измерить adaptive posting container | PERF | candidate | M | G-PERF |
| 794 | LIN-0909 | [D23](#d23) | Ограничить per-document token count | CORRECTNESS | qualification | M | G-CORRECT |
| 795 | LIN-0910 | [D23](#d23) | Ограничить unique vocabulary bytes | CORRECTNESS | qualification | M | G-CORRECT |
| 796 | LIN-0911 | [D23](#d23) | Ограничить pending FTS bytes | CORRECTNESS | qualification | M | G-CORRECT |
| 797 | LIN-0912 | [D23](#d23) | Ограничить term length allocation | CORRECTNESS | qualification | M | G-CORRECT |
| 798 | LIN-0913 | [D23](#d23) | Проверить FTS cold rebuild parity | CORRECTNESS | qualification | S | G-CORRECT |
| 799 | LIN-0914 | [D23](#d23) | Проверить FTS serde rebuild parity | CORRECTNESS | qualification | S | G-CORRECT |
| 800 | LIN-0915 | [D23](#d23) | Проверить SSO text tokenizer parity | CORRECTNESS | qualification | S | G-CORRECT |
| 801 | LIN-0916 | [D23](#d23) | Проверить binary-invalid text rejection | CORRECTNESS | qualification | S | G-CORRECT |
| 802 | LIN-0917 | [D23](#d23) | Добавить tokenizer inspection API | FEATURE | candidate | M | G-FEATURE |
| 803 | LIN-0918 | [D23](#d23) | Добавить vocabulary statistics API | FEATURE | candidate | M | G-FEATURE |
| 804 | LIN-0919 | [D23](#d23) | Добавить FTS build progress | FEATURE | candidate | M | G-FEATURE |
| 805 | LIN-0920 | [D23](#d23) | Добавить tokenizer regression corpus | VERIFY | qualification | S | G-VERIFY |
| 806 | LIN-0921 | [D24](#d24) | Проверить borrowed posting view lifetime | CORRECTNESS | qualification | S | G-CORRECT |
| 807 | LIN-0922 | [D24](#d24) | Проверить zero-copy single-term query | CORRECTNESS | qualification | S | G-CORRECT |
| 808 | LIN-0923 | [D24](#d24) | Проверить multiword posting intersection | CORRECTNESS | qualification | S | G-CORRECT |
| 809 | LIN-0924 | [D24](#d24) | Проверить multiword posting union | CORRECTNESS | qualification | S | G-CORRECT |
| 810 | LIN-0925 | [D24](#d24) | Измерить shortest-list-first intersection | PERF | candidate | M | G-PERF |
| 811 | LIN-0926 | [D24](#d24) | Измерить galloping posting intersection | PERF | candidate | M | G-PERF |
| 812 | LIN-0927 | [D24](#d24) | Измерить bitmap/list adaptive intersection | PERF | candidate | M | G-PERF |
| 813 | LIN-0928 | [D24](#d24) | Измерить query token cache | PERF | candidate | M | G-PERF |
| 814 | LIN-0929 | [D24](#d24) | Проверить frequent-term Top-K exactness | CORRECTNESS | qualification | S | G-CORRECT |
| 815 | LIN-0930 | [D24](#d24) | Проверить selective-term ranking exactness | CORRECTNESS | qualification | S | G-CORRECT |
| 816 | LIN-0931 | [D24](#d24) | Проверить miss-query empty output | CORRECTNESS | qualification | S | G-CORRECT |
| 817 | LIN-0932 | [D24](#d24) | Проверить repeated-query-term weighting | CORRECTNESS | qualification | S | G-CORRECT |
| 818 | LIN-0933 | [D24](#d24) | Спроектировать phrase search | FEATURE | candidate | L | G-FEATURE |
| 819 | LIN-0934 | [D24](#d24) | Спроектировать proximity search | FEATURE | candidate | L | G-FEATURE |
| 820 | LIN-0935 | [D24](#d24) | Спроектировать prefix term queries | FEATURE | candidate | L | G-FEATURE |
| 821 | LIN-0936 | [D24](#d24) | Спроектировать fuzzy term opt-in | VERIFY | qualification | L | G-VERIFY |
| 822 | LIN-0937 | [D24](#d24) | Спроектировать field-boost scoring | FEATURE | candidate | L | G-FEATURE |
| 823 | LIN-0938 | [D24](#d24) | Спроектировать BM25 opt-in | FEATURE | candidate | L | G-FEATURE |
| 824 | LIN-0939 | [D24](#d24) | Спроектировать score explanation API | FEATURE | candidate | L | G-FEATURE |
| 825 | LIN-0940 | [D24](#d24) | Спроектировать score model versioning | FEATURE | candidate | L | G-FEATURE |
| 826 | LIN-0941 | [D24](#d24) | Проверить lexical score tie ordering | CORRECTNESS | qualification | S | G-CORRECT |
| 827 | LIN-0942 | [D24](#d24) | Проверить hybrid candidate completeness | CORRECTNESS | qualification | S | G-CORRECT |
| 828 | LIN-0943 | [D24](#d24) | Проверить hybrid RRF exactness | CORRECTNESS | qualification | S | G-CORRECT |
| 829 | LIN-0944 | [D24](#d24) | Проверить hybrid bounded result equivalence | CORRECTNESS | qualification | S | G-CORRECT |
| 830 | LIN-0965 | [D25](#d25) | Проверить move-row posting remap | CORRECTNESS | qualification | S | G-CORRECT |
| 831 | LIN-0966 | [D25](#d25) | Проверить swap-delete posting repair | CORRECTNESS | qualification | S | G-CORRECT |
| 832 | LIN-0967 | [D25](#d25) | Проверить delete-reinsert same identity | CORRECTNESS | qualification | S | G-CORRECT |
| 833 | LIN-0968 | [D25](#d25) | Проверить canceled FTS mutation | CORRECTNESS | qualification | S | G-CORRECT |
| 834 | LIN-0969 | [D25](#d25) | Проверить pending add/delete cancellation | CORRECTNESS | qualification | S | G-CORRECT |
| 835 | LIN-0970 | [D25](#d25) | Проверить fold duplicate elimination | CORRECTNESS | qualification | S | G-CORRECT |
| 836 | LIN-0971 | [D25](#d25) | Проверить monotonic posting order | CORRECTNESS | qualification | S | G-CORRECT |
| 837 | LIN-0972 | [D25](#d25) | Проверить no resurrected documents | CORRECTNESS | qualification | S | G-CORRECT |
| 838 | LIN-0973 | [D25](#d25) | Измерить pending fold trigger | PERF | candidate | M | G-PERF |
| 839 | LIN-0974 | [D25](#d25) | Измерить incremental fold work | PERF | candidate | M | G-PERF |
| 840 | LIN-0975 | [D25](#d25) | Измерить compaction pause budget | PERF | candidate | M | G-PERF |
| 841 | LIN-0976 | [D25](#d25) | Измерить hot-term mutation cost | PERF | candidate | M | G-PERF |
| 842 | LIN-0977 | [D25](#d25) | Спроектировать term-local generations | FEATURE | candidate | L | G-FEATURE |
| 843 | LIN-0978 | [D25](#d25) | Спроектировать tombstone density policy | FEATURE | candidate | L | G-FEATURE |
| 844 | LIN-0979 | [D25](#d25) | Спроектировать incremental posting compaction | FEATURE | candidate | L | G-FEATURE |
| 845 | LIN-0980 | [D25](#d25) | Спроектировать compaction backpressure | FEATURE | candidate | L | G-FEATURE |
| 846 | LIN-0981 | [D25](#d25) | Проверить rollback pending vocabulary | CORRECTNESS | qualification | S | G-CORRECT |
| 847 | LIN-0982 | [D25](#d25) | Проверить rollback term removals | CORRECTNESS | qualification | S | G-CORRECT |
| 848 | LIN-0983 | [D25](#d25) | Проверить bulk update token diffs | CORRECTNESS | qualification | S | G-CORRECT |
| 849 | LIN-0984 | [D25](#d25) | Проверить repeated no-op FTS updates | CORRECTNESS | qualification | S | G-CORRECT |
| 850 | LIN-0985 | [D25](#d25) | Ограничить pending edits per term | CORRECTNESS | qualification | M | G-CORRECT |
| 851 | LIN-0986 | [D25](#d25) | Ограничить stale posting retention | CORRECTNESS | qualification | M | G-CORRECT |
| 852 | LIN-0987 | [D25](#d25) | Ограничить compaction temporary memory | CORRECTNESS | qualification | M | G-CORRECT |
| 853 | LIN-0988 | [D25](#d25) | Ограничить pathological hot-term churn | CORRECTNESS | qualification | M | G-CORRECT |
| 854 | LIN-0989 | [D25](#d25) | Добавить FTS invariant checker | FEATURE | candidate | M | G-FEATURE |
| 855 | LIN-0990 | [D25](#d25) | Добавить live-versus-rebuild diff tool | FEATURE | candidate | M | G-FEATURE |
| 856 | LIN-0991 | [D25](#d25) | Добавить term mutation counters | FEATURE | candidate | M | G-FEATURE |
| 857 | LIN-0992 | [D25](#d25) | Добавить compaction debt telemetry | FEATURE | candidate | M | G-FEATURE |
| 858 | LIN-0993 | [D25](#d25) | Сравнить shared scratch на ASCII | PERF | candidate | M | G-PERF |
| 859 | LIN-0994 | [D25](#d25) | Сравнить shared scratch на Unicode | PERF | candidate | M | G-PERF |
| 860 | LIN-0995 | [D25](#d25) | Сравнить posting delta representation | PERF | candidate | M | G-PERF |
| 861 | LIN-0996 | [D25](#d25) | Сравнить batched term remapping | PERF | candidate | M | G-PERF |
| 862 | LIN-0997 | [D25](#d25) | Проверить mutation после checkpoint | CORRECTNESS | qualification | S | G-CORRECT |
| 863 | LIN-0998 | [D25](#d25) | Проверить mutation после replay | CORRECTNESS | qualification | S | G-CORRECT |
| 864 | LIN-0999 | [D25](#d25) | Проверить mutation после schema FTS change | CORRECTNESS | qualification | S | G-CORRECT |
| 865 | LIN-1000 | [D25](#d25) | Проверить mutation при concurrent readers | CORRECTNESS | qualification | S | G-CORRECT |
| 866 | LIN-1001 | [D26](#d26) | Профилировать current embed_batch | PERF | candidate | M | G-PERF |
| 867 | LIN-1002 | [D26](#d26) | Профилировать embedding text assembly | PERF | candidate | M | G-PERF |
| 868 | LIN-1003 | [D26](#d26) | Профилировать scratch finish allocation | PERF | candidate | M | G-PERF |
| 869 | LIN-1004 | [D26](#d26) | Профилировать feature hashing instructions | PERF | candidate | M | G-PERF |
| 870 | LIN-1005 | [D26](#d26) | Проверить embedder identity persistence | CORRECTNESS | qualification | S | G-CORRECT |
| 871 | LIN-1006 | [D26](#d26) | Проверить dimension parsing limits | CORRECTNESS | qualification | S | G-CORRECT |
| 872 | LIN-1007 | [D26](#d26) | Проверить minimum dimension policy | CORRECTNESS | qualification | S | G-CORRECT |
| 873 | LIN-1008 | [D26](#d26) | Проверить empty text embedding | CORRECTNESS | qualification | S | G-CORRECT |
| 874 | LIN-1009 | [D26](#d26) | Проверить ASCII/Unicode feature parity | CORRECTNESS | qualification | S | G-CORRECT |
| 875 | LIN-1010 | [D26](#d26) | Проверить whole-text bigram semantics | CORRECTNESS | qualification | S | G-CORRECT |
| 876 | LIN-1011 | [D26](#d26) | Проверить token trigram semantics | CORRECTNESS | qualification | S | G-CORRECT |
| 877 | LIN-1012 | [D26](#d26) | Проверить repeated-token normalization | CORRECTNESS | qualification | S | G-CORRECT |
| 878 | LIN-1013 | [D26](#d26) | Проверить dense/sparse normalization bits | CORRECTNESS | qualification | S | G-CORRECT |
| 879 | LIN-1014 | [D26](#d26) | Проверить touched slot reset | CORRECTNESS | qualification | S | G-CORRECT |
| 880 | LIN-1015 | [D26](#d26) | Проверить long-input sum fallback | CORRECTNESS | qualification | S | G-CORRECT |
| 881 | LIN-1016 | [D26](#d26) | Проверить variable-dimension slot mapping | CORRECTNESS | qualification | S | G-CORRECT |
| 882 | LIN-1017 | [D26](#d26) | Измерить reusable embedding output buffers | PERF | candidate | M | G-PERF |
| 883 | LIN-1018 | [D26](#d26) | Измерить direct Arc slice construction | PERF | candidate | M | G-PERF |
| 884 | LIN-1019 | [D26](#d26) | Измерить trigram cache working set | PERF | candidate | M | G-PERF |
| 885 | LIN-1020 | [D26](#d26) | Измерить batch-local feature cache | PERF | candidate | M | G-PERF |
| 886 | LIN-1021 | [D26](#d26) | Измерить warm thread-pool embedding | PERF | candidate | M | G-PERF |
| 887 | LIN-1022 | [D26](#d26) | Измерить cold thread-pool startup | PERF | candidate | M | G-PERF |
| 888 | LIN-1023 | [D26](#d26) | Измерить scoped embedding crossover | PERF | candidate | M | G-PERF |
| 889 | LIN-1024 | [D26](#d26) | Измерить concurrent embedding contention | PERF | candidate | M | G-PERF |
| 890 | LIN-1045 | [D27](#d27) | Проверить HNSW insertion ordering | CORRECTNESS | qualification | L | G-CORRECT |
| 891 | LIN-1046 | [D27](#d27) | Проверить HNSW duplicate identity handling | CORRECTNESS | qualification | L | G-CORRECT |
| 892 | LIN-1047 | [D27](#d27) | Проверить HNSW row deletion repair | CORRECTNESS | qualification | L | G-CORRECT |
| 893 | LIN-1048 | [D27](#d27) | Проверить HNSW swap-remove repair | CORRECTNESS | qualification | L | G-CORRECT |
| 894 | LIN-1049 | [D27](#d27) | Проверить HNSW embedding updates | CORRECTNESS | qualification | L | G-CORRECT |
| 895 | LIN-1050 | [D27](#d27) | Проверить HNSW collection isolation | CORRECTNESS | qualification | L | G-CORRECT |
| 896 | LIN-1051 | [D27](#d27) | Проверить HNSW dimension changes | CORRECTNESS | qualification | L | G-CORRECT |
| 897 | LIN-1052 | [D27](#d27) | Проверить HNSW model identity changes | CORRECTNESS | qualification | L | G-CORRECT |
| 898 | LIN-1053 | [D27](#d27) | Спроектировать configurable ef_search | FEATURE | candidate | L | G-FEATURE |
| 899 | LIN-1054 | [D27](#d27) | Спроектировать configurable ef_construction | FEATURE | candidate | L | G-FEATURE |
| 900 | LIN-1055 | [D27](#d27) | Спроектировать configurable neighbor degree | FEATURE | candidate | L | G-FEATURE |
| 901 | LIN-1056 | [D27](#d27) | Спроектировать deterministic HNSW seed | FEATURE | candidate | L | G-FEATURE |
| 902 | LIN-1057 | [D27](#d27) | Проверить SQ8 zero-vector quantization | CORRECTNESS | qualification | S | G-CORRECT |
| 903 | LIN-1058 | [D27](#d27) | Проверить SQ8 clipping boundaries | CORRECTNESS | qualification | S | G-CORRECT |
| 904 | LIN-1059 | [D27](#d27) | Проверить SQ8 negative coordinates | CORRECTNESS | qualification | S | G-CORRECT |
| 905 | LIN-1060 | [D27](#d27) | Проверить SQ8 NaN/infinity policy | CORRECTNESS | qualification | S | G-CORRECT |
| 906 | LIN-1061 | [D27](#d27) | Измерить SQ8 recall degradation | PERF | candidate | M | G-PERF |
| 907 | LIN-1062 | [D27](#d27) | Измерить exact reranking budget | PERF | candidate | M | G-PERF |
| 908 | LIN-1063 | [D27](#d27) | Измерить filtered ANN recall | PERF | candidate | L | G-PERF |
| 909 | LIN-1064 | [D27](#d27) | Измерить skewed-cluster HNSW behavior | PERF | candidate | L | G-PERF |
| 910 | LIN-1085 | [D28](#d28) | Проверить CPU f64 accumulation order | CORRECTNESS | qualification | S | G-CORRECT |
| 911 | LIN-1086 | [D28](#d28) | Проверить vector threshold границу 0.01 | CORRECTNESS | qualification | S | G-CORRECT |
| 912 | LIN-1087 | [D28](#d28) | Проверить vector score tie ordering | CORRECTNESS | qualification | S | G-CORRECT |
| 913 | LIN-1088 | [D28](#d28) | Проверить duplicate vector identity ordering | CORRECTNESS | qualification | S | G-CORRECT |
| 914 | LIN-1089 | [D28](#d28) | Проверить exact Top-K completeness | CORRECTNESS | qualification | S | G-CORRECT |
| 915 | LIN-1090 | [D28](#d28) | Проверить filtered exact Top-K | CORRECTNESS | qualification | S | G-CORRECT |
| 916 | LIN-1091 | [D28](#d28) | Проверить skip/take vector search | CORRECTNESS | qualification | S | G-CORRECT |
| 917 | LIN-1092 | [D28](#d28) | Проверить vector query empty corpus | VERIFY | qualification | S | G-VERIFY |
| 918 | LIN-1093 | [D28](#d28) | Проверить hybrid missing modality | CORRECTNESS | qualification | S | G-CORRECT |
| 919 | LIN-1094 | [D28](#d28) | Проверить hybrid rank offsets | CORRECTNESS | qualification | S | G-CORRECT |
| 920 | LIN-1095 | [D28](#d28) | Проверить hybrid duplicate candidate IDs | CORRECTNESS | qualification | S | G-CORRECT |
| 921 | LIN-1096 | [D28](#d28) | Проверить hybrid score bit stability | CORRECTNESS | qualification | S | G-CORRECT |
| 922 | LIN-1097 | [D28](#d28) | Спроектировать explicit distance metric | FEATURE | candidate | L | G-FEATURE |
| 923 | LIN-1098 | [D28](#d28) | Спроектировать dot-product search | FEATURE | candidate | L | G-FEATURE |
| 924 | LIN-1099 | [D28](#d28) | Спроектировать Euclidean search | FEATURE | candidate | L | G-FEATURE |
| 925 | LIN-1100 | [D28](#d28) | Спроектировать normalized-vector metadata | FEATURE | candidate | L | G-FEATURE |
| 926 | LIN-1101 | [D28](#d28) | Добавить query-time vector parameters | FEATURE | candidate | M | G-FEATURE |
| 927 | LIN-1102 | [D28](#d28) | Добавить vector field selection | FEATURE | candidate | M | G-FEATURE |
| 928 | LIN-1103 | [D28](#d28) | Добавить multi-vector row search | FEATURE | candidate | M | G-FEATURE |
| 929 | LIN-1104 | [D28](#d28) | Добавить vector search explanation | FEATURE | candidate | M | G-FEATURE |
| 930 | LIN-1125 | [D29](#d29) | Проверить resident cache generation keys | CORRECTNESS | done | S | G-CORRECT |
| 931 | LIN-1126 | [D29](#d29) | Проверить vector cache model identity | CORRECTNESS | done | S | G-CORRECT |
| 932 | LIN-1127 | [D29](#d29) | Проверить vector cache row ordering | CORRECTNESS | done | S | G-CORRECT |
| 933 | LIN-1128 | [D29](#d29) | Проверить cache invalidation после mutation | CORRECTNESS | done | S | G-CORRECT |
| 934 | LIN-1129 | [D29](#d29) | Ограничить resident GPU bytes | CORRECTNESS | qualification | M | G-CORRECT |
| 935 | LIN-1130 | [D29](#d29) | Ограничить storage buffer sizes | CORRECTNESS | qualification | M | G-CORRECT |
| 936 | LIN-1131 | [D29](#d29) | Ограничить bind-group count | CORRECTNESS | qualification | M | G-CORRECT |
| 937 | LIN-1132 | [D29](#d29) | Ограничить dispatch dimensions | CORRECTNESS | qualification | M | G-CORRECT |
| 938 | LIN-1133 | [D29](#d29) | Проверить readback map failure | CORRECTNESS | qualification | S | G-CORRECT |
| 939 | LIN-1134 | [D29](#d29) | Проверить mapped buffer cancellation | CORRECTNESS | qualification | S | G-CORRECT |
| 940 | LIN-1135 | [D29](#d29) | Проверить staging buffer reuse | CORRECTNESS | done | S | G-CORRECT |
| 941 | LIN-1136 | [D29](#d29) | Проверить dropped GPU query cleanup | CORRECTNESS | qualification | S | G-CORRECT |
| 942 | LIN-1137 | [D29](#d29) | Измерить upload/readback split | PERF | candidate | M | G-PERF |
| 943 | LIN-1138 | [D29](#d29) | Измерить resident repeated query benefit | PERF | candidate | M | G-PERF |
| 944 | LIN-1139 | [D29](#d29) | Измерить CPU/GPU crossover cardinality | PERF | candidate | M | G-PERF |
| 945 | LIN-1140 | [D29](#d29) | Измерить GPU command encoding cost | PERF | candidate | M | G-PERF |
| 946 | LIN-1141 | [D29](#d29) | Спроектировать buffer pool policy | FEATURE | done | L | G-FEATURE |
| 947 | LIN-1142 | [D29](#d29) | Спроектировать shared resident columns | FEATURE | candidate | L | G-FEATURE |
| 948 | LIN-1143 | [D29](#d29) | Спроектировать resident intermediate batches | FEATURE | candidate | L | G-FEATURE |
| 949 | LIN-1144 | [D29](#d29) | Спроектировать explicit residency handles | FEATURE | candidate | L | G-FEATURE |
| 950 | LIN-1165 | [D30](#d30) | Проверить GPU Unicode substring cases | VERIFY | qualification | S | G-VERIFY |
| 951 | LIN-1166 | [D30](#d30) | Проверить GPU UTF-8 equality cases | VERIFY | qualification | S | G-VERIFY |
| 952 | LIN-1167 | [D30](#d30) | Проверить GPU word boundary cases | VERIFY | qualification | S | G-VERIFY |
| 953 | LIN-1168 | [D30](#d30) | Проверить GPU lowercase metadata parity | CORRECTNESS | qualification | S | G-CORRECT |
| 954 | LIN-1169 | [D30](#d30) | Проверить regex empty-match semantics | CORRECTNESS | qualification | S | G-CORRECT |
| 955 | LIN-1170 | [D30](#d30) | Проверить regex anchor semantics | CORRECTNESS | qualification | S | G-CORRECT |
| 956 | LIN-1171 | [D30](#d30) | Проверить regex look assertion metadata | CORRECTNESS | qualification | S | G-CORRECT |
| 957 | LIN-1172 | [D30](#d30) | Проверить regex resource exhaustion | CORRECTNESS | qualification | S | G-CORRECT |
| 958 | LIN-1173 | [D30](#d30) | Проверить GPU grouped count overflow | CORRECTNESS | qualification | S | G-CORRECT |
| 959 | LIN-1174 | [D30](#d30) | Проверить binary64 sum carry paths | CORRECTNESS | qualification | S | G-CORRECT |
| 960 | LIN-1175 | [D30](#d30) | Проверить binary64 NaN canonicalization | CORRECTNESS | qualification | S | G-CORRECT |
| 961 | LIN-1176 | [D30](#d30) | Проверить exceptional-value sum ordering | CORRECTNESS | qualification | S | G-CORRECT |
| 962 | LIN-1177 | [D30](#d30) | Проверить GPU graph reverse traversal | CORRECTNESS | qualification | S | G-CORRECT |
| 963 | LIN-1178 | [D30](#d30) | Проверить GPU graph unresolved nodes | CORRECTNESS | qualification | S | G-CORRECT |
| 964 | LIN-1179 | [D30](#d30) | Проверить GPU graph cycle termination | CORRECTNESS | qualification | S | G-CORRECT |
| 965 | LIN-1180 | [D30](#d30) | Проверить GPU DFS output order | CORRECTNESS | qualification | S | G-CORRECT |
| 966 | LIN-1181 | [D30](#d30) | Измерить predicate kernel fusion | PERF | candidate | M | G-PERF |
| 967 | LIN-1182 | [D30](#d30) | Измерить projection kernel fusion | PERF | candidate | M | G-PERF |
| 968 | LIN-1183 | [D30](#d30) | Измерить resident join intermediates | PERF | candidate | M | G-PERF |
| 969 | LIN-1184 | [D30](#d30) | Измерить resident aggregation intermediates | PERF | candidate | M | G-PERF |
| 970 | LIN-1201 | [D31](#d31) | Проверить BFS visited identity policy | CORRECTNESS | qualification | S | G-CORRECT |
| 971 | LIN-1202 | [D31](#d31) | Проверить DFS path multiplicity | CORRECTNESS | qualification | S | G-CORRECT |
| 972 | LIN-1203 | [D31](#d31) | Проверить reverse-edge traversal | CORRECTNESS | qualification | S | G-CORRECT |
| 973 | LIN-1204 | [D31](#d31) | Проверить dangling-node traversal | CORRECTNESS | qualification | S | G-CORRECT |
| 974 | LIN-1205 | [D31](#d31) | Проверить self-loop graph behavior | CORRECTNESS | qualification | S | G-CORRECT |
| 975 | LIN-1206 | [D31](#d31) | Проверить multi-edge graph behavior | CORRECTNESS | qualification | S | G-CORRECT |
| 976 | LIN-1207 | [D31](#d31) | Проверить cycle depth boundaries | CORRECTNESS | qualification | S | G-CORRECT |
| 977 | LIN-1208 | [D31](#d31) | Проверить unresolved endpoint behavior | CORRECTNESS | qualification | S | G-CORRECT |
| 978 | LIN-1209 | [D31](#d31) | Измерить adjacency map locality | PERF | candidate | M | G-PERF |
| 979 | LIN-1210 | [D31](#d31) | Измерить for_each_neighbor borrowing | PERF | candidate | M | G-PERF |
| 980 | LIN-1211 | [D31](#d31) | Измерить traversal scratch reuse | PERF | candidate | M | G-PERF |
| 981 | LIN-1212 | [D31](#d31) | Измерить sparse visited representation | PERF | candidate | M | G-PERF |
| 982 | LIN-1213 | [D31](#d31) | Спроектировать edge property schema | FEATURE | candidate | L | G-FEATURE |
| 983 | LIN-1214 | [D31](#d31) | Спроектировать labeled path predicates | FEATURE | candidate | L | G-FEATURE |
| 984 | LIN-1215 | [D31](#d31) | Спроектировать shortest unweighted paths | FEATURE | candidate | L | G-FEATURE |
| 985 | LIN-1216 | [D31](#d31) | Спроектировать weighted path opt-in | FEATURE | candidate | L | G-FEATURE |
| 986 | LIN-1217 | [D31](#d31) | Добавить traversal edge filters | FEATURE | candidate | M | G-FEATURE |
| 987 | LIN-1218 | [D31](#d31) | Добавить traversal node filters | FEATURE | candidate | M | G-FEATURE |
| 988 | LIN-1219 | [D31](#d31) | Добавить traversal result projections | FEATURE | candidate | M | G-FEATURE |
| 989 | LIN-1220 | [D31](#d31) | Добавить graph result explanation | FEATURE | candidate | M | G-FEATURE |
| 990 | LIN-1221 | [D31](#d31) | Ограничить traversal visited count | CORRECTNESS | qualification | M | G-CORRECT |
| 991 | LIN-1222 | [D31](#d31) | Ограничить traversal frontier bytes | CORRECTNESS | qualification | M | G-CORRECT |
| 992 | LIN-1223 | [D31](#d31) | Ограничить traversal emitted paths | CORRECTNESS | qualification | M | G-CORRECT |
| 993 | LIN-1224 | [D31](#d31) | Ограничить graph recursion depth | CORRECTNESS | qualification | M | G-CORRECT |
| 994 | LIN-1241 | [D32](#d32) | Профилировать cold page faults | PERF | candidate | M | G-PERF |
| 995 | LIN-1242 | [D32](#d32) | Профилировать cold decode allocations | PERF | candidate | M | G-PERF |
| 996 | LIN-1243 | [D32](#d32) | Профилировать cold projection access | PERF | candidate | M | G-PERF |
| 997 | LIN-1244 | [D32](#d32) | Профилировать cold index rebuilding | PERF | candidate | M | G-PERF |
| 998 | LIN-1245 | [D32](#d32) | Проверить mmap lifetime pinning | CORRECTNESS | qualification | S | G-CORRECT |
| 999 | LIN-1246 | [D32](#d32) | Проверить file replacement mapped readers | CORRECTNESS | qualification | S | G-CORRECT |
| 1000 | LIN-1247 | [D32](#d32) | Проверить mmap truncation handling | CORRECTNESS | qualification | S | G-CORRECT |
| 1001 | LIN-1248 | [D32](#d32) | Проверить cold buffer alias lifetime | CORRECTNESS | qualification | S | G-CORRECT |
| 1002 | LIN-1249 | [D32](#d32) | Измерить sequential madvise policy | PERF | candidate | M | G-PERF |
| 1003 | LIN-1250 | [D32](#d32) | Измерить random madvise policy | PERF | candidate | M | G-PERF |
| 1004 | LIN-1251 | [D32](#d32) | Измерить bounded prefetch windows | PERF | candidate | M | G-PERF |
| 1005 | LIN-1252 | [D32](#d32) | Измерить page-aligned section layout | PERF | candidate | M | G-PERF |
| 1006 | LIN-1253 | [D32](#d32) | Спроектировать chunked cold columns | FEATURE | candidate | L | G-FEATURE |
| 1007 | LIN-1254 | [D32](#d32) | Спроектировать lazy field decoding | FEATURE | candidate | L | G-FEATURE |
| 1008 | LIN-1255 | [D32](#d32) | Спроектировать hot/cold tier thresholds | FEATURE | candidate | L | G-FEATURE |
| 1009 | LIN-1256 | [D32](#d32) | Спроектировать cold section checksums | FEATURE | candidate | L | G-FEATURE |
| 1010 | LIN-1257 | [D32](#d32) | Проверить corrupted cold offsets | CORRECTNESS | qualification | S | G-CORRECT |
| 1011 | LIN-1258 | [D32](#d32) | Проверить oversized cold sections | CORRECTNESS | qualification | S | G-CORRECT |
| 1012 | LIN-1259 | [D32](#d32) | Проверить missing cold backing file | CORRECTNESS | qualification | S | G-CORRECT |
| 1013 | LIN-1260 | [D32](#d32) | Проверить cold file permission errors | CORRECTNESS | qualification | S | G-CORRECT |
| 1014 | LIN-1261 | [D32](#d32) | Измерить dictionary cold strings | PERF | candidate | M | G-PERF |
| 1015 | LIN-1262 | [D32](#d32) | Измерить delta-encoded cold integers | PERF | candidate | M | G-PERF |
| 1016 | LIN-1263 | [D32](#d32) | Измерить compressed cold vectors | PERF | candidate | M | G-PERF |
| 1017 | LIN-1264 | [D32](#d32) | Измерить compressed block random access | PERF | candidate | M | G-PERF |
| 1018 | LIN-1281 | [D33](#d33) | Проверить spawn_blocking ownership | CORRECTNESS | qualification | S | G-CORRECT |
| 1019 | LIN-1282 | [D33](#d33) | Проверить cancellation после queue admission | CORRECTNESS | qualification | S | G-CORRECT |
| 1020 | LIN-1283 | [D33](#d33) | Проверить cancellation во время scan | CORRECTNESS | qualification | S | G-CORRECT |
| 1021 | LIN-1284 | [D33](#d33) | Проверить cancellation во время streaming | CORRECTNESS | qualification | S | G-CORRECT |
| 1022 | LIN-1285 | [D33](#d33) | Ограничить blocking worker concurrency | CORRECTNESS | qualification | M | G-CORRECT |
| 1023 | LIN-1286 | [D33](#d33) | Ограничить queued query count | CORRECTNESS | qualification | M | G-CORRECT |
| 1024 | LIN-1287 | [D33](#d33) | Ограничить queued input bytes | CORRECTNESS | qualification | M | G-CORRECT |
| 1025 | LIN-1288 | [D33](#d33) | Ограничить stream buffered rows | CORRECTNESS | qualification | M | G-CORRECT |
| 1026 | LIN-1289 | [D33](#d33) | Спроектировать per-client backpressure | API | candidate | L | G-API |
| 1027 | LIN-1290 | [D33](#d33) | Спроектировать deadline propagation | API | candidate | L | G-API |
| 1028 | LIN-1291 | [D33](#d33) | Спроектировать priority admission policy | API | candidate | L | G-API |
| 1029 | LIN-1292 | [D33](#d33) | Спроектировать graceful server draining | API | candidate | L | G-API |
| 1030 | LIN-1293 | [D33](#d33) | Измерить first-item async latency | PERF | candidate | M | G-PERF |
| 1031 | LIN-1294 | [D33](#d33) | Измерить async batch coalescing | PERF | candidate | M | G-PERF |
| 1032 | LIN-1295 | [D33](#d33) | Измерить scheduler handoff overhead | PERF | candidate | M | G-PERF |
| 1033 | LIN-1296 | [D33](#d33) | Измерить stream wakeup amplification | PERF | candidate | M | G-PERF |
| 1034 | LIN-1297 | [D33](#d33) | Проверить dropped future cleanup | CORRECTNESS | qualification | S | G-CORRECT |
| 1035 | LIN-1298 | [D33](#d33) | Проверить dropped receiver cleanup | CORRECTNESS | qualification | S | G-CORRECT |
| 1036 | LIN-1299 | [D33](#d33) | Проверить slow consumer memory bound | CORRECTNESS | qualification | S | G-CORRECT |
| 1037 | LIN-1300 | [D33](#d33) | Проверить canceled writer transaction rollback | CORRECTNESS | qualification | S | G-CORRECT |
| 1038 | LIN-1301 | [D33](#d33) | Добавить stream completion receipts | API | candidate | M | G-API |
| 1039 | LIN-1302 | [D33](#d33) | Добавить async queue wait metrics | API | candidate | M | G-API |
| 1040 | LIN-1303 | [D33](#d33) | Добавить active-query gauges | API | candidate | M | G-API |
| 1041 | LIN-1304 | [D33](#d33) | Добавить cancellation reason counters | API | candidate | M | G-API |
| 1042 | LIN-1325 | [D34](#d34) | Проверить snapshot graph consistency | CORRECTNESS | qualification | S | G-CORRECT |
| 1043 | LIN-1326 | [D34](#d34) | Проверить snapshot vector cache isolation | CORRECTNESS | qualification | S | G-CORRECT |
| 1044 | LIN-1327 | [D34](#d34) | Проверить catalog consistency изолированных ReadDb | CORRECTNESS | qualification | S | G-CORRECT |
| 1045 | LIN-1328 | [D34](#d34) | Проверить snapshot после failed write | CORRECTNESS | qualification | S | G-CORRECT |
| 1046 | LIN-1329 | [D34](#d34) | Измерить read snapshot creation | PERF | candidate | M | G-PERF |
| 1047 | LIN-1330 | [D34](#d34) | Измерить snapshot retained memory | PERF | candidate | M | G-PERF |
| 1048 | LIN-1331 | [D34](#d34) | Измерить Arc contention concurrent readers | PERF | candidate | M | G-PERF |
| 1049 | LIN-1332 | [D34](#d34) | Измерить cross-thread query scaling | PERF | candidate | M | G-PERF |
| 1050 | LIN-1333 | [D34](#d34) | Спроектировать immutable segment sharing | FEATURE | candidate | L | G-FEATURE |
| 1051 | LIN-1334 | [D34](#d34) | Спроектировать generation-based reclamation | FEATURE | candidate | L | G-FEATURE |
| 1052 | LIN-1335 | [D34](#d34) | Спроектировать reader lifetime diagnostics | FEATURE | candidate | L | G-FEATURE |
| 1053 | LIN-1336 | [D34](#d34) | Спроектировать writer admission fairness | FEATURE | candidate | L | G-FEATURE |
| 1054 | LIN-1337 | [D34](#d34) | Проверить many readers one writer | CORRECTNESS | qualification | S | G-CORRECT |
| 1055 | LIN-1338 | [D34](#d34) | Проверить long reader checkpoint pressure | CORRECTNESS | qualification | S | G-CORRECT |
| 1056 | LIN-1339 | [D34](#d34) | Проверить dropped reader reclamation | CORRECTNESS | qualification | S | G-CORRECT |
| 1057 | LIN-1340 | [D34](#d34) | Проверить reader panic isolation | CORRECTNESS | qualification | S | G-CORRECT |
| 1058 | LIN-1341 | [D34](#d34) | Добавить snapshot pin byte counters | FEATURE | candidate | M | G-FEATURE |
| 1059 | LIN-1342 | [D34](#d34) | Добавить oldest-reader generation metric | FEATURE | candidate | M | G-FEATURE |
| 1060 | LIN-1343 | [D34](#d34) | Добавить blocked-reclamation diagnostics | FEATURE | candidate | M | G-FEATURE |
| 1061 | LIN-1344 | [D34](#d34) | Добавить concurrent read latency histograms | FEATURE | candidate | M | G-FEATURE |
| 1062 | LIN-1365 | [D35](#d35) | Проверить TCP_NODELAY effectiveness | CORRECTNESS | qualification | S | G-CORRECT |
| 1063 | LIN-1366 | [D35](#d35) | Измерить single-write handshake cost | PERF | candidate | M | G-PERF |
| 1064 | LIN-1367 | [D35](#d35) | Измерить replication RTT budget | PERF | candidate | M | G-PERF |
| 1065 | LIN-1368 | [D35](#d35) | Измерить replication throughput saturation | PERF | candidate | M | G-PERF |
| 1066 | LIN-1369 | [D35](#d35) | Проверить TLS hostname mismatch | CORRECTNESS | qualification | S | G-CORRECT |
| 1067 | LIN-1370 | [D35](#d35) | Проверить expired certificate rejection | CORRECTNESS | qualification | S | G-CORRECT |
| 1068 | LIN-1371 | [D35](#d35) | Проверить missing certificate rejection | CORRECTNESS | qualification | S | G-CORRECT |
| 1069 | LIN-1372 | [D35](#d35) | Проверить untrusted CA rejection | CORRECTNESS | qualification | S | G-CORRECT |
| 1070 | LIN-1373 | [D35](#d35) | Спроектировать token rotation overlap | FEATURE | candidate | L | G-FEATURE |
| 1071 | LIN-1374 | [D35](#d35) | Спроектировать replication authorization scopes | FEATURE | candidate | L | G-FEATURE |
| 1072 | LIN-1375 | [D35](#d35) | Спроектировать authenticated peer identity | FEATURE | candidate | L | G-FEATURE |
| 1073 | LIN-1376 | [D35](#d35) | Спроектировать replay-resistant session nonce | FEATURE | candidate | L | G-FEATURE |
| 1074 | LIN-1377 | [D35](#d35) | Ограничить incoming frame size | CORRECTNESS | qualification | M | G-CORRECT |
| 1075 | LIN-1378 | [D35](#d35) | Ограничить handshake read duration | CORRECTNESS | qualification | M | G-CORRECT |
| 1076 | LIN-1379 | [D35](#d35) | Ограничить idle connection lifetime | CORRECTNESS | qualification | M | G-CORRECT |
| 1077 | LIN-1380 | [D35](#d35) | Ограничить active replication clients | CORRECTNESS | qualification | M | G-CORRECT |
| 1078 | LIN-1381 | [D35](#d35) | Проверить partial network reads | CORRECTNESS | qualification | S | G-CORRECT |
| 1079 | LIN-1382 | [D35](#d35) | Проверить partial network writes | CORRECTNESS | qualification | S | G-CORRECT |
| 1080 | LIN-1383 | [D35](#d35) | Проверить connection reset handling | CORRECTNESS | qualification | S | G-CORRECT |
| 1081 | LIN-1384 | [D35](#d35) | Проверить half-closed socket behavior | CORRECTNESS | qualification | S | G-CORRECT |
| 1082 | LIN-1405 | [D36](#d36) | Проверить follower read-only enforcement | CORRECTNESS | qualification | S | G-CORRECT |
| 1083 | LIN-1406 | [D36](#d36) | Проверить follower local mutation rejection | CORRECTNESS | qualification | S | G-CORRECT |
| 1084 | LIN-1407 | [D36](#d36) | Проверить follower catalog propagation | CORRECTNESS | qualification | S | G-CORRECT |
| 1085 | LIN-1408 | [D36](#d36) | Проверить follower index rebuild policy | CORRECTNESS | qualification | S | G-CORRECT |
| 1086 | LIN-1409 | [D36](#d36) | Проверить checkpoint transition catch-up | CORRECTNESS | qualification | S | G-CORRECT |
| 1087 | LIN-1410 | [D36](#d36) | Проверить WAL rotation catch-up | CORRECTNESS | qualification | S | G-CORRECT |
| 1088 | LIN-1411 | [D36](#d36) | Проверить source restart reconnect | CORRECTNESS | qualification | S | G-CORRECT |
| 1089 | LIN-1412 | [D36](#d36) | Проверить follower restart reconnect | CORRECTNESS | qualification | S | G-CORRECT |
| 1090 | LIN-1413 | [D36](#d36) | Спроектировать snapshot bootstrap protocol | FEATURE | candidate | L | G-FEATURE |
| 1091 | LIN-1414 | [D36](#d36) | Спроектировать bounded replay catch-up | FEATURE | candidate | L | G-FEATURE |
| 1092 | LIN-1415 | [D36](#d36) | Спроектировать catch-up progress receipts | FEATURE | candidate | L | G-FEATURE |
| 1093 | LIN-1416 | [D36](#d36) | Спроектировать stale follower detection | FEATURE | candidate | L | G-FEATURE |
| 1094 | LIN-1417 | [D36](#d36) | Проверить follower disk-full behavior | CORRECTNESS | qualification | S | G-CORRECT |
| 1095 | LIN-1418 | [D36](#d36) | Проверить follower corrupted local WAL | CORRECTNESS | qualification | S | G-CORRECT |
| 1096 | LIN-1419 | [D36](#d36) | Проверить follower partial transfer recovery | CORRECTNESS | qualification | S | G-CORRECT |
| 1097 | LIN-1420 | [D36](#d36) | Проверить follower version incompatibility | CORRECTNESS | qualification | S | G-CORRECT |
| 1098 | LIN-1421 | [D36](#d36) | Спроектировать retention pinned by followers | FEATURE | candidate | L | G-FEATURE |
| 1099 | LIN-1422 | [D36](#d36) | Спроектировать follower lag admission | FEATURE | candidate | L | G-FEATURE |
| 1100 | LIN-1423 | [D36](#d36) | Спроектировать read consistency modes | FEATURE | candidate | L | G-FEATURE |
| 1101 | LIN-1424 | [D36](#d36) | Спроектировать explicit promotion fencing | FEATURE | candidate | L | G-FEATURE |
| 1102 | LIN-1441 | [D37](#d37) | Зафиксировать stable Row representation | CORRECTNESS | qualification | M | G-CORRECT |
| 1103 | LIN-1442 | [D37](#d37) | Зафиксировать public Cell variant policy | CORRECTNESS | qualification | M | G-CORRECT |
| 1104 | LIN-1443 | [D37](#d37) | Зафиксировать query result ownership | CORRECTNESS | qualification | M | G-CORRECT |
| 1105 | LIN-1444 | [D37](#d37) | Зафиксировать prepared handle invalidation | CORRECTNESS | qualification | M | G-CORRECT |
| 1106 | LIN-1445 | [D37](#d37) | Спроектировать DbBuilder configuration | API | candidate | L | G-API |
| 1107 | LIN-1446 | [D37](#d37) | Спроектировать resource-limit configuration | API | candidate | L | G-API |
| 1108 | LIN-1447 | [D37](#d37) | Спроектировать storage capability introspection | API | candidate | L | G-API |
| 1109 | LIN-1448 | [D37](#d37) | Спроектировать backend capability introspection | API | candidate | L | G-API |
| 1110 | LIN-1449 | [D37](#d37) | Добавить explicit query cancellation handle | API | candidate | M | G-API |
| 1111 | LIN-1450 | [D37](#d37) | Добавить query deadline builder | API | candidate | M | G-API |
| 1112 | LIN-1451 | [D37](#d37) | Добавить bounded result collection | API | candidate | M | G-API |
| 1113 | LIN-1452 | [D37](#d37) | Добавить streamed typed row conversion | API | candidate | M | G-API |
| 1114 | LIN-1453 | [D37](#d37) | Проверить public error type stability | CORRECTNESS | qualification | S | G-CORRECT |
| 1115 | LIN-1454 | [D37](#d37) | Проверить public serde compatibility | CORRECTNESS | qualification | S | G-CORRECT |
| 1116 | LIN-1455 | [D37](#d37) | Проверить no-default-features usability | CORRECTNESS | qualification | S | G-CORRECT |
| 1117 | LIN-1456 | [D37](#d37) | Проверить minimum Rust version usability | CORRECTNESS | qualification | S | G-CORRECT |
| 1118 | LIN-1457 | [D37](#d37) | Спроектировать safe borrowed row API | API | candidate | L | G-API |
| 1119 | LIN-1458 | [D37](#d37) | Спроектировать typed parameter binding | API | candidate | L | G-API |
| 1120 | LIN-1459 | [D37](#d37) | Спроектировать prepared query schema info | API | candidate | L | G-API |
| 1121 | LIN-1460 | [D37](#d37) | Спроектировать query execution receipts | API | candidate | L | G-API |
| 1122 | LIN-1461 | [D37](#d37) | Добавить exact/approximate search options | API | candidate | M | G-API |
| 1123 | LIN-1462 | [D37](#d37) | Добавить explicit durability options | API | candidate | M | G-API |
| 1124 | LIN-1463 | [D37](#d37) | Добавить explain structured output | API | candidate | M | G-API |
| 1125 | LIN-1464 | [D37](#d37) | Добавить structured runtime statistics | API | candidate | M | G-API |
| 1126 | LIN-1481 | [D38](#d38) | Проверить derive field rename | CORRECTNESS | qualification | S | G-CORRECT |
| 1127 | LIN-1482 | [D38](#d38) | Проверить derive optional fields | CORRECTNESS | qualification | S | G-CORRECT |
| 1128 | LIN-1483 | [D38](#d38) | Проверить derive missing required fields | CORRECTNESS | qualification | S | G-CORRECT |
| 1129 | LIN-1484 | [D38](#d38) | Проверить derive unknown field policy | CORRECTNESS | qualification | S | G-CORRECT |
| 1130 | LIN-1485 | [D38](#d38) | Проверить derive scalar numeric bounds | CORRECTNESS | qualification | S | G-CORRECT |
| 1131 | LIN-1486 | [D38](#d38) | Проверить derive timestamp conversion | CORRECTNESS | qualification | S | G-CORRECT |
| 1132 | LIN-1487 | [D38](#d38) | Проверить derive vector conversion | CORRECTNESS | qualification | S | G-CORRECT |
| 1133 | LIN-1488 | [D38](#d38) | Проверить derive SmallText conversion | CORRECTNESS | qualification | S | G-CORRECT |
| 1134 | LIN-1489 | [D38](#d38) | Улучшить proc-macro error spans | API | candidate | M | G-API |
| 1135 | LIN-1490 | [D38](#d38) | Уточнить unsupported field-type errors | API | candidate | M | G-API |
| 1136 | LIN-1491 | [D38](#d38) | Уточнить conflicting attribute errors | API | candidate | M | G-API |
| 1137 | LIN-1492 | [D38](#d38) | Уточнить duplicate field rename errors | API | candidate | M | G-API |
| 1138 | LIN-1493 | [D38](#d38) | Спроектировать typed parameter structs | API | candidate | L | G-API |
| 1139 | LIN-1494 | [D38](#d38) | Спроектировать typed projection schemas | API | candidate | L | G-API |
| 1140 | LIN-1495 | [D38](#d38) | Спроектировать typed aggregate results | API | candidate | L | G-API |
| 1141 | LIN-1496 | [D38](#d38) | Спроектировать typed join result mapping | API | candidate | L | G-API |
| 1142 | LIN-1497 | [D38](#d38) | Добавить borrowed typed row views | API | candidate | M | G-API |
| 1143 | LIN-1498 | [D38](#d38) | Добавить zero-copy text borrowing | API | candidate | M | G-API |
| 1144 | LIN-1499 | [D38](#d38) | Добавить typed RecordBatch conversion | API | candidate | M | G-API |
| 1145 | LIN-1500 | [D38](#d38) | Добавить typed async stream mapping | API | candidate | M | G-API |
| 1146 | LIN-1501 | [D38](#d38) | Проверить generic struct derive | CORRECTNESS | qualification | S | G-CORRECT |
| 1147 | LIN-1502 | [D38](#d38) | Проверить nested module derive paths | CORRECTNESS | qualification | S | G-CORRECT |
| 1148 | LIN-1503 | [D38](#d38) | Проверить renamed crate dependency | CORRECTNESS | qualification | S | G-CORRECT |
| 1149 | LIN-1504 | [D38](#d38) | Проверить derive feature-disabled diagnostics | CORRECTNESS | qualification | S | G-CORRECT |
| 1150 | LIN-1521 | [D39](#d39) | Уточнить CLI subcommand help | API | candidate | M | G-API |
| 1151 | LIN-1522 | [D39](#d39) | Уточнить invalid option diagnostics | API | candidate | M | G-API |
| 1152 | LIN-1523 | [D39](#d39) | Уточнить missing argument diagnostics | API | candidate | M | G-API |
| 1153 | LIN-1524 | [D39](#d39) | Уточнить unsupported backend diagnostics | API | candidate | M | G-API |
| 1154 | LIN-1525 | [D39](#d39) | Добавить machine-readable CLI errors | API | candidate | M | G-API |
| 1155 | LIN-1526 | [D39](#d39) | Добавить stable process exit codes | API | candidate | M | G-API |
| 1156 | LIN-1527 | [D39](#d39) | Добавить JSON Lines output mode | API | candidate | M | G-API |
| 1157 | LIN-1528 | [D39](#d39) | Добавить tabular output truncation policy | API | candidate | M | G-API |
| 1158 | LIN-1529 | [D39](#d39) | Спроектировать interactive REPL | API | candidate | L | G-API |
| 1159 | LIN-1530 | [D39](#d39) | Спроектировать multiline REPL input | API | candidate | L | G-API |
| 1160 | LIN-1531 | [D39](#d39) | Спроектировать REPL history storage | API | candidate | L | G-API |
| 1161 | LIN-1532 | [D39](#d39) | Спроектировать REPL completion metadata | API | candidate | L | G-API |
| 1162 | LIN-1533 | [D39](#d39) | Добавить query file execution | API | candidate | M | G-API |
| 1163 | LIN-1534 | [D39](#d39) | Добавить parameter file binding | API | candidate | M | G-API |
| 1164 | LIN-1535 | [D39](#d39) | Добавить stdin streaming import | API | candidate | M | G-API |
| 1165 | LIN-1536 | [D39](#d39) | Добавить stdout streaming export | API | candidate | M | G-API |
| 1166 | LIN-1537 | [D39](#d39) | Добавить dry-run plan command | API | candidate | M | G-API |
| 1167 | LIN-1538 | [D39](#d39) | Добавить storage inspect command | API | candidate | M | G-API |
| 1168 | LIN-1539 | [D39](#d39) | Добавить WAL inspect command | API | candidate | M | G-API |
| 1169 | LIN-1540 | [D39](#d39) | Добавить index consistency command | API | candidate | M | G-API |
| 1170 | LIN-1541 | [D39](#d39) | Проверить SIGINT query cancellation | CORRECTNESS | qualification | S | G-CORRECT |
| 1171 | LIN-1542 | [D39](#d39) | Проверить SIGTERM graceful shutdown | CORRECTNESS | qualification | S | G-CORRECT |
| 1172 | LIN-1543 | [D39](#d39) | Проверить broken-pipe output handling | CORRECTNESS | qualification | S | G-CORRECT |
| 1173 | LIN-1544 | [D39](#d39) | Проверить noninteractive prompt suppression | CORRECTNESS | qualification | S | G-CORRECT |
| 1174 | LIN-1561 | [D40](#d40) | Спроектировать stable diagnostic codes | FEATURE | candidate | L | G-FEATURE |
| 1175 | LIN-1562 | [D40](#d40) | Спроектировать error cause chains | FEATURE | candidate | L | G-FEATURE |
| 1176 | LIN-1563 | [D40](#d40) | Спроектировать source-span attachments | FEATURE | candidate | L | G-FEATURE |
| 1177 | LIN-1564 | [D40](#d40) | Спроектировать operation context attachments | FEATURE | candidate | L | G-FEATURE |
| 1178 | LIN-1565 | [D40](#d40) | Различать corrupt и unsupported format | CORRECTNESS | qualification | M | G-CORRECT |
| 1179 | LIN-1566 | [D40](#d40) | Различать not-found и missing-schema | CORRECTNESS | qualification | M | G-CORRECT |
| 1180 | LIN-1567 | [D40](#d40) | Различать canceled и deadline-exceeded | CORRECTNESS | qualification | M | G-CORRECT |
| 1181 | LIN-1568 | [D40](#d40) | Различать resource-limit и allocation-failure | CORRECTNESS | qualification | M | G-CORRECT |
| 1182 | LIN-1569 | [D40](#d40) | Проверить nested error source preservation | CORRECTNESS | qualification | S | G-CORRECT |
| 1183 | LIN-1570 | [D40](#d40) | Проверить I/O path redaction | CORRECTNESS | qualification | S | G-CORRECT |
| 1184 | LIN-1571 | [D40](#d40) | Проверить token redaction in network errors | CORRECTNESS | qualification | S | G-CORRECT |
| 1185 | LIN-1572 | [D40](#d40) | Проверить SQL credential redaction | CORRECTNESS | qualification | S | G-CORRECT |
| 1186 | LIN-1573 | [D40](#d40) | Добавить constraint identity in errors | FEATURE | candidate | M | G-FEATURE |
| 1187 | LIN-1574 | [D40](#d40) | Добавить collection/field error context | FEATURE | candidate | M | G-FEATURE |
| 1188 | LIN-1575 | [D40](#d40) | Добавить failing WAL byte offset | FEATURE | candidate | M | G-FEATURE |
| 1189 | LIN-1576 | [D40](#d40) | Добавить malformed vector coordinate offset | FEATURE | candidate | M | G-FEATURE |
| 1190 | LIN-1577 | [D40](#d40) | Улучшить actionable recovery suggestions | CORRECTNESS | qualification | M | G-CORRECT |
| 1191 | LIN-1578 | [D40](#d40) | Уточнить missing feature remediation | CORRECTNESS | qualification | M | G-CORRECT |
| 1192 | LIN-1579 | [D40](#d40) | Уточнить stale prepared handle remediation | CORRECTNESS | qualification | M | G-CORRECT |
| 1193 | LIN-1580 | [D40](#d40) | Уточнить incompatible model remediation | CORRECTNESS | qualification | M | G-CORRECT |
| 1194 | LIN-1581 | [D40](#d40) | Проверить error display без panic | CORRECTNESS | qualification | S | G-CORRECT |
| 1195 | LIN-1582 | [D40](#d40) | Проверить invalid UTF-8 error rendering | CORRECTNESS | qualification | S | G-CORRECT |
| 1196 | LIN-1583 | [D40](#d40) | Проверить huge value diagnostic truncation | CORRECTNESS | qualification | S | G-CORRECT |
| 1197 | LIN-1584 | [D40](#d40) | Проверить cyclic error source defense | CORRECTNESS | qualification | S | G-CORRECT |
| 1198 | LIN-1601 | [D41](#d41) | Добавить per-query execution ID | OPS | candidate | M | G-OPS |
| 1199 | LIN-1602 | [D41](#d41) | Добавить query phase timing schema | OPS | candidate | M | G-OPS |
| 1200 | LIN-1603 | [D41](#d41) | Добавить operator row-flow counters | OPS | candidate | M | G-OPS |
| 1201 | LIN-1604 | [D41](#d41) | Добавить operator allocated-byte counters | OPS | candidate | M | G-OPS |
| 1202 | LIN-1605 | [D41](#d41) | Добавить prepare latency histogram | OPS | candidate | M | G-OPS |
| 1203 | LIN-1606 | [D41](#d41) | Добавить execute latency histogram | OPS | candidate | M | G-OPS |
| 1204 | LIN-1607 | [D41](#d41) | Добавить commit latency histogram | OPS | candidate | M | G-OPS |
| 1205 | LIN-1608 | [D41](#d41) | Добавить queue latency histogram | OPS | candidate | M | G-OPS |
| 1206 | LIN-1609 | [D41](#d41) | Добавить WAL pending bytes gauge | OPS | candidate | M | G-OPS |
| 1207 | LIN-1610 | [D41](#d41) | Добавить checkpoint debt gauge | OPS | candidate | M | G-OPS |
| 1208 | LIN-1611 | [D41](#d41) | Добавить memory pool retained gauge | OPS | candidate | M | G-OPS |
| 1209 | LIN-1612 | [D41](#d41) | Добавить resident GPU bytes gauge | OPS | candidate | M | G-OPS |
| 1210 | LIN-1613 | [D41](#d41) | Спроектировать tracing opt-in integration | OPS | candidate | L | G-OPS |
| 1211 | LIN-1614 | [D41](#d41) | Спроектировать metrics sink trait | OPS | candidate | L | G-OPS |
| 1212 | LIN-1615 | [D41](#d41) | Спроектировать OpenTelemetry adapter | OPS | candidate | L | G-OPS |
| 1213 | LIN-1616 | [D41](#d41) | Спроектировать Prometheus text exporter | OPS | candidate | L | G-OPS |
| 1214 | LIN-1617 | [D41](#d41) | Проверить no-op instrumentation overhead | CORRECTNESS | qualification | S | G-CORRECT |
| 1215 | LIN-1618 | [D41](#d41) | Проверить telemetry cardinality limits | CORRECTNESS | qualification | S | G-CORRECT |
| 1216 | LIN-1619 | [D41](#d41) | Проверить query text redaction | CORRECTNESS | qualification | S | G-CORRECT |
| 1217 | LIN-1620 | [D41](#d41) | Проверить high-cardinality label rejection | CORRECTNESS | qualification | S | G-CORRECT |
| 1218 | LIN-1621 | [D41](#d41) | Добавить slow-query ring buffer | OPS | candidate | M | G-OPS |
| 1219 | LIN-1622 | [D41](#d41) | Добавить bounded query sampling | OPS | candidate | M | G-OPS |
| 1220 | LIN-1623 | [D41](#d41) | Добавить phase anomaly detection hints | OPS | candidate | M | G-OPS |
| 1221 | LIN-1624 | [D41](#d41) | Добавить plan-hash correlation | OPS | candidate | M | G-OPS |
| 1222 | LIN-1645 | [D42](#d42) | Генерировать indexed/scan equivalence cases | VERIFY | qualification | S | G-VERIFY |
| 1223 | LIN-1646 | [D42](#d42) | Генерировать optimized/reference plan cases | VERIFY | qualification | S | G-VERIFY |
| 1224 | LIN-1647 | [D42](#d42) | Генерировать live/rebuilt FTS cases | VERIFY | qualification | S | G-VERIFY |
| 1225 | LIN-1648 | [D42](#d42) | Генерировать restored/live Store cases | VERIFY | qualification | S | G-VERIFY |
| 1226 | LIN-1649 | [D42](#d42) | Генерировать mixed numeric key corpus | VERIFY | qualification | S | G-VERIFY |
| 1227 | LIN-1650 | [D42](#d42) | Генерировать Text/SmallText scalar corpus | VERIFY | qualification | S | G-VERIFY |
| 1228 | LIN-1651 | [D42](#d42) | Генерировать NaN payload corpus | VERIFY | qualification | S | G-VERIFY |
| 1229 | LIN-1652 | [D42](#d42) | Генерировать Unicode normalization corpus | VERIFY | qualification | S | G-VERIFY |
| 1230 | LIN-1653 | [D42](#d42) | Добавить mutation sequence shrinking | VERIFY | qualification | S | G-VERIFY |
| 1231 | LIN-1654 | [D42](#d42) | Добавить WAL corruption shrinking | VERIFY | qualification | S | G-VERIFY |
| 1232 | LIN-1655 | [D42](#d42) | Добавить graph failure shrinking | VERIFY | qualification | S | G-VERIFY |
| 1233 | LIN-1656 | [D42](#d42) | Добавить SQL peer mismatch shrinking | VERIFY | qualification | S | G-VERIFY |
| 1234 | LIN-1657 | [D42](#d42) | Проверить aggregation against independent oracle | VERIFY | qualification | S | G-VERIFY |
| 1235 | LIN-1658 | [D42](#d42) | Проверить windows against naive oracle | VERIFY | qualification | S | G-VERIFY |
| 1236 | LIN-1659 | [D42](#d42) | Проверить sorting against total-order oracle | VERIFY | qualification | S | G-VERIFY |
| 1237 | LIN-1660 | [D42](#d42) | Проверить Top-K against full-rank oracle | VERIFY | qualification | S | G-VERIFY |
| 1238 | LIN-1661 | [D42](#d42) | Проверить graph against adjacency reference | VERIFY | qualification | S | G-VERIFY |
| 1239 | LIN-1662 | [D42](#d42) | Проверить joins against nested-loop oracle | VERIFY | qualification | S | G-VERIFY |
| 1240 | LIN-1663 | [D42](#d42) | Проверить regex against CPU reference | VERIFY | qualification | S | G-VERIFY |
| 1241 | LIN-1664 | [D42](#d42) | Проверить sparse vectors against scalar encoder | VERIFY | qualification | S | G-VERIFY |
| 1242 | LIN-1685 | [D43](#d43) | Инъецировать crash перед durable sync | VERIFY | qualification | S | G-VERIFY |
| 1243 | LIN-1686 | [D43](#d43) | Инъецировать crash после durable sync | VERIFY | qualification | S | G-VERIFY |
| 1244 | LIN-1687 | [D43](#d43) | Инъецировать crash перед commit acknowledgement | VERIFY | qualification | S | G-VERIFY |
| 1245 | LIN-1688 | [D43](#d43) | Инъецировать crash после acknowledgement | VERIFY | qualification | S | G-VERIFY |
| 1246 | LIN-1689 | [D43](#d43) | Инъецировать crash перед snapshot rename | VERIFY | qualification | S | G-VERIFY |
| 1247 | LIN-1690 | [D43](#d43) | Инъецировать crash после snapshot rename | VERIFY | qualification | S | G-VERIFY |
| 1248 | LIN-1691 | [D43](#d43) | Инъецировать crash перед WAL truncate | VERIFY | qualification | S | G-VERIFY |
| 1249 | LIN-1692 | [D43](#d43) | Инъецировать crash после WAL truncate | VERIFY | qualification | S | G-VERIFY |
| 1250 | LIN-1693 | [D43](#d43) | Инъецировать ENOSPC при append | VERIFY | qualification | S | G-VERIFY |
| 1251 | LIN-1694 | [D43](#d43) | Инъецировать EIO при sync | VERIFY | qualification | S | G-VERIFY |
| 1252 | LIN-1695 | [D43](#d43) | Инъецировать EINTR при writev | VERIFY | qualification | S | G-VERIFY |
| 1253 | LIN-1696 | [D43](#d43) | Инъецировать EMFILE при reopen | VERIFY | qualification | S | G-VERIFY |
| 1254 | LIN-1697 | [D43](#d43) | Инъецировать network drop при bootstrap | VERIFY | qualification | S | G-VERIFY |
| 1255 | LIN-1698 | [D43](#d43) | Инъецировать network drop при catch-up | VERIFY | qualification | S | G-VERIFY |
| 1256 | LIN-1699 | [D43](#d43) | Инъецировать TLS failure при reconnect | VERIFY | qualification | S | G-VERIFY |
| 1257 | LIN-1700 | [D43](#d43) | Инъецировать slow replica acknowledgement | VERIFY | qualification | S | G-VERIFY |
| 1258 | LIN-1702 | [D43](#d43) | Проверить no unacknowledged partial transaction | VERIFY | qualification | S | G-VERIFY |
| 1259 | LIN-1703 | [D43](#d43) | Проверить no duplicate replay effects | VERIFY | qualification | S | G-VERIFY |
| 1260 | LIN-1704 | [D43](#d43) | Проверить recovery idempotence after faults | VERIFY | qualification | S | G-VERIFY |
| 1261 | LIN-1721 | [D44](#d44) | Зафиксировать Rust toolchain policy | CORRECTNESS | qualification | M | G-CORRECT |
| 1262 | LIN-1722 | [D44](#d44) | Проверять declared MSRV build | OPS | candidate | M | G-OPS |
| 1263 | LIN-1723 | [D44](#d44) | Проверять stable toolchain build | OPS | candidate | M | G-OPS |
| 1264 | LIN-1724 | [D44](#d44) | Проверять nightly-specific feature use | OPS | candidate | M | G-OPS |
| 1265 | LIN-1725 | [D44](#d44) | Добавить Linux x86_64 test job | OPS | candidate | M | G-OPS |
| 1266 | LIN-1726 | [D44](#d44) | Добавить Linux ARM64 test job | OPS | candidate | M | G-OPS |
| 1267 | LIN-1727 | [D44](#d44) | Добавить macOS ARM64 test job | OPS | candidate | M | G-OPS |
| 1268 | LIN-1728 | [D44](#d44) | Добавить Windows x86_64 test job | OPS | candidate | M | G-OPS |
| 1269 | LIN-1729 | [D44](#d44) | Проверять default feature set | OPS | candidate | M | G-OPS |
| 1270 | LIN-1730 | [D44](#d44) | Проверять no-default feature set | OPS | candidate | M | G-OPS |
| 1271 | LIN-1731 | [D44](#d44) | Проверять async-only compatibility | OPS | candidate | M | G-OPS |
| 1272 | LIN-1732 | [D44](#d44) | Проверять derive-only compatibility | OPS | candidate | M | G-OPS |
| 1273 | LIN-1733 | [D44](#d44) | Проверять gpu compile-only job | OPS | candidate | M | G-OPS |
| 1274 | LIN-1734 | [D44](#d44) | Проверять actual GPU qualification job | OPS | candidate | M | G-OPS |
| 1275 | LIN-1735 | [D44](#d44) | Проверять compression feature matrix | OPS | candidate | M | G-OPS |
| 1276 | LIN-1736 | [D44](#d44) | Проверять parallel feature matrix | OPS | candidate | M | G-OPS |
| 1277 | LIN-1737 | [D44](#d44) | Разделить deterministic и noisy benchmarks | OPS | candidate | M | G-OPS |
| 1278 | LIN-1738 | [D44](#d44) | Разделить smoke и regression suites | OPS | candidate | M | G-OPS |
| 1279 | LIN-1739 | [D44](#d44) | Разделить component и end-to-end gates | OPS | candidate | M | G-OPS |
| 1280 | LIN-1740 | [D44](#d44) | Разделить hardware-required tests | OPS | candidate | M | G-OPS |
| 1281 | LIN-1741 | [D44](#d44) | Добавить raw benchmark artifact upload | OPS | candidate | M | G-OPS |
| 1282 | LIN-1742 | [D44](#d44) | Добавить benchmark source hash checks | OPS | candidate | M | G-OPS |
| 1283 | LIN-1743 | [D44](#d44) | Добавить dependency lock drift checks | OPS | candidate | M | G-OPS |
| 1284 | LIN-1744 | [D44](#d44) | Добавить baseline approval metadata | OPS | candidate | M | G-OPS |
| 1285 | LIN-1765 | [D45](#d45) | Ограничить regex automaton size | CORRECTNESS | qualification | M | G-CORRECT |
| 1286 | LIN-1766 | [D45](#d45) | Ограничить graph expansion count | CORRECTNESS | qualification | M | G-CORRECT |
| 1287 | LIN-1767 | [D45](#d45) | Ограничить Cartesian join expansion | CORRECTNESS | qualification | M | G-CORRECT |
| 1288 | LIN-1768 | [D45](#d45) | Ограничить aggregate state bytes | CORRECTNESS | qualification | M | G-CORRECT |
| 1289 | LIN-1769 | [D45](#d45) | Проверить archive path traversal rejection | CORRECTNESS | qualification | S | G-CORRECT |
| 1290 | LIN-1770 | [D45](#d45) | Проверить backup symlink policy | CORRECTNESS | qualification | S | G-CORRECT |
| 1291 | LIN-1771 | [D45](#d45) | Проверить data-directory traversal policy | CORRECTNESS | qualification | S | G-CORRECT |
| 1292 | LIN-1772 | [D45](#d45) | Проверить temp-file permission policy | CORRECTNESS | qualification | S | G-CORRECT |
| 1293 | LIN-1773 | [D45](#d45) | Проверить replication token comparison | CORRECTNESS | qualification | S | G-CORRECT |
| 1294 | LIN-1774 | [D45](#d45) | Проверить secret lifetime in diagnostics | CORRECTNESS | qualification | S | G-CORRECT |
| 1295 | LIN-1775 | [D45](#d45) | Проверить HTTP remote TLS requirement | CORRECTNESS | qualification | S | G-CORRECT |
| 1296 | LIN-1776 | [D45](#d45) | Проверить accidental insecure endpoint reuse | CORRECTNESS | qualification | S | G-CORRECT |
| 1297 | LIN-1777 | [D45](#d45) | Спроектировать read-only capabilities | FEATURE | candidate | L | G-FEATURE |
| 1298 | LIN-1778 | [D45](#d45) | Спроектировать collection-level capabilities | FEATURE | candidate | L | G-FEATURE |
| 1299 | LIN-1779 | [D45](#d45) | Спроектировать extension trust boundaries | FEATURE | candidate | L | G-FEATURE |
| 1300 | LIN-1780 | [D45](#d45) | Спроектировать untrusted model isolation | FEATURE | candidate | L | G-FEATURE |
| 1301 | LIN-1781 | [D45](#d45) | Проверить compressed payload bomb rejection | CORRECTNESS | qualification | S | G-CORRECT |
| 1302 | LIN-1782 | [D45](#d45) | Проверить nested format recursion limits | CORRECTNESS | qualification | S | G-CORRECT |
| 1303 | LIN-1783 | [D45](#d45) | Проверить pathological UTF-8 scan cost | CORRECTNESS | qualification | S | G-CORRECT |
| 1304 | LIN-1784 | [D45](#d45) | Проверить adversarial hash collision handling | CORRECTNESS | qualification | S | G-CORRECT |
| 1305 | LIN-1801 | [D46](#d46) | Спроектировать schema generation counters | FEATURE | candidate | L | G-FEATURE |
| 1306 | LIN-1802 | [D46](#d46) | Спроектировать explicit schema version | FEATURE | candidate | L | G-FEATURE |
| 1307 | LIN-1803 | [D46](#d46) | Спроектировать migration transaction contract | FEATURE | candidate | L | G-FEATURE |
| 1308 | LIN-1804 | [D46](#d46) | Спроектировать migration dry-run output | FEATURE | candidate | L | G-FEATURE |
| 1309 | LIN-1805 | [D46](#d46) | Проверить collection rename references | CORRECTNESS | qualification | S | G-CORRECT |
| 1310 | LIN-1806 | [D46](#d46) | Проверить field rename index references | CORRECTNESS | qualification | S | G-CORRECT |
| 1311 | LIN-1807 | [D46](#d46) | Проверить field rename FTS references | CORRECTNESS | qualification | S | G-CORRECT |
| 1312 | LIN-1808 | [D46](#d46) | Проверить field rename prepared plans | CORRECTNESS | qualification | S | G-CORRECT |
| 1313 | LIN-1809 | [D46](#d46) | Спроектировать add-field default policy | FEATURE | candidate | L | G-FEATURE |
| 1314 | LIN-1810 | [D46](#d46) | Спроектировать remove-field dependency analysis | FEATURE | candidate | L | G-FEATURE |
| 1315 | LIN-1811 | [D46](#d46) | Спроектировать field type conversion policy | FEATURE | candidate | L | G-FEATURE |
| 1316 | LIN-1812 | [D46](#d46) | Спроектировать nullable-to-required migration | FEATURE | candidate | L | G-FEATURE |
| 1317 | LIN-1813 | [D46](#d46) | Проверить index metadata migration | CORRECTNESS | qualification | S | G-CORRECT |
| 1318 | LIN-1814 | [D46](#d46) | Проверить vector model metadata migration | CORRECTNESS | qualification | S | G-CORRECT |
| 1319 | LIN-1815 | [D46](#d46) | Проверить graph relation metadata migration | CORRECTNESS | qualification | S | G-CORRECT |
| 1320 | LIN-1816 | [D46](#d46) | Проверить persisted statistics migration | CORRECTNESS | qualification | S | G-CORRECT |
| 1321 | LIN-1817 | [D46](#d46) | Добавить catalog introspection API | FEATURE | candidate | M | G-FEATURE |
| 1322 | LIN-1818 | [D46](#d46) | Добавить schema diff operation | FEATURE | candidate | M | G-FEATURE |
| 1323 | LIN-1819 | [D46](#d46) | Добавить schema compatibility report | FEATURE | candidate | M | G-FEATURE |
| 1324 | LIN-1820 | [D46](#d46) | Добавить migration progress callbacks | FEATURE | candidate | M | G-FEATURE |
| 1325 | LIN-1821 | [D46](#d46) | Спроектировать online index rebuild | FEATURE | candidate | L | G-FEATURE |
| 1326 | LIN-1822 | [D46](#d46) | Спроектировать staged constraint validation | FEATURE | candidate | L | G-FEATURE |
| 1327 | LIN-1823 | [D46](#d46) | Спроектировать migration checkpoint boundaries | FEATURE | candidate | L | G-FEATURE |
| 1328 | LIN-1824 | [D46](#d46) | Спроектировать resumable migration receipts | FEATURE | candidate | L | G-FEATURE |
| 1329 | LIN-1841 | [D47](#d47) | Добавить bounded JSON Lines import | FEATURE | candidate | M | G-FEATURE |
| 1330 | LIN-1842 | [D47](#d47) | Добавить bounded CSV import | FEATURE | candidate | M | G-FEATURE |
| 1331 | LIN-1843 | [D47](#d47) | Добавить typed RecordBatch import | FEATURE | candidate | M | G-FEATURE |
| 1332 | LIN-1844 | [D47](#d47) | Добавить Arrow import integration | FEATURE | candidate | M | G-FEATURE |
| 1333 | LIN-1845 | [D47](#d47) | Добавить streaming JSON Lines export | FEATURE | candidate | M | G-FEATURE |
| 1334 | LIN-1846 | [D47](#d47) | Добавить streaming CSV export | FEATURE | candidate | M | G-FEATURE |
| 1335 | LIN-1847 | [D47](#d47) | Добавить Arrow export integration | FEATURE | candidate | M | G-FEATURE |
| 1336 | LIN-1848 | [D47](#d47) | Добавить portable logical backup export | FEATURE | candidate | M | G-FEATURE |
| 1337 | LIN-1849 | [D47](#d47) | Проверить CSV quotes/newlines | CORRECTNESS | qualification | S | G-CORRECT |
| 1338 | LIN-1850 | [D47](#d47) | Проверить CSV empty/null distinction | CORRECTNESS | qualification | S | G-CORRECT |
| 1339 | LIN-1851 | [D47](#d47) | Проверить JSON large integer preservation | CORRECTNESS | qualification | S | G-CORRECT |
| 1340 | LIN-1852 | [D47](#d47) | Проверить JSON nonfinite float policy | CORRECTNESS | qualification | S | G-CORRECT |
| 1341 | LIN-1853 | [D47](#d47) | Спроектировать import schema inference | FEATURE | candidate | L | G-FEATURE |
| 1342 | LIN-1854 | [D47](#d47) | Спроектировать explicit import schema mapping | FEATURE | candidate | L | G-FEATURE |
| 1343 | LIN-1855 | [D47](#d47) | Спроектировать import field rename mapping | FEATURE | candidate | L | G-FEATURE |
| 1344 | LIN-1856 | [D47](#d47) | Спроектировать import default mapping | FEATURE | candidate | L | G-FEATURE |
| 1345 | LIN-1857 | [D47](#d47) | Добавить import reject-file mode | FEATURE | candidate | M | G-FEATURE |
| 1346 | LIN-1858 | [D47](#d47) | Добавить import error row positions | FEATURE | candidate | M | G-FEATURE |
| 1347 | LIN-1859 | [D47](#d47) | Добавить import progress byte counts | FEATURE | candidate | M | G-FEATURE |
| 1348 | LIN-1860 | [D47](#d47) | Добавить import cancellation receipts | FEATURE | candidate | M | G-FEATURE |
| 1349 | LIN-1861 | [D47](#d47) | Проверить imported duplicate IDs | CORRECTNESS | qualification | S | G-CORRECT |
| 1350 | LIN-1862 | [D47](#d47) | Проверить imported duplicate URIs | CORRECTNESS | qualification | S | G-CORRECT |
| 1351 | LIN-1863 | [D47](#d47) | Проверить imported FK violations | CORRECTNESS | qualification | S | G-CORRECT |
| 1352 | LIN-1864 | [D47](#d47) | Проверить imported vector dimensions | CORRECTNESS | qualification | S | G-CORRECT |
| 1353 | LIN-1881 | [D48](#d48) | Спроектировать health command contract | OPS | candidate | L | G-OPS |
| 1354 | LIN-1882 | [D48](#d48) | Спроектировать readiness command contract | OPS | candidate | L | G-OPS |
| 1355 | LIN-1883 | [D48](#d48) | Спроектировать graceful shutdown sequence | OPS | candidate | L | G-OPS |
| 1356 | LIN-1884 | [D48](#d48) | Спроектировать storage lock inspection | OPS | candidate | L | G-OPS |
| 1357 | LIN-1885 | [D48](#d48) | Проверить multiple-writer lock refusal | CORRECTNESS | qualification | S | G-CORRECT |
| 1358 | LIN-1886 | [D48](#d48) | Проверить stale lock recovery policy | CORRECTNESS | qualification | S | G-CORRECT |
| 1359 | LIN-1887 | [D48](#d48) | Проверить lock error diagnostics | CORRECTNESS | qualification | S | G-CORRECT |
| 1360 | LIN-1888 | [D48](#d48) | Проверить lock release после panic | CORRECTNESS | qualification | S | G-CORRECT |
| 1361 | LIN-1889 | [D48](#d48) | Добавить disk-space preflight checks | OPS | candidate | M | G-OPS |
| 1362 | LIN-1890 | [D48](#d48) | Добавить checkpoint scheduling policy | OPS | candidate | M | G-OPS |
| 1363 | LIN-1891 | [D48](#d48) | Добавить WAL retention budget | OPS | candidate | M | G-OPS |
| 1364 | LIN-1892 | [D48](#d48) | Добавить cold-tier capacity accounting | OPS | candidate | M | G-OPS |
| 1365 | LIN-1893 | [D48](#d48) | Спроектировать backup retention manifests | OPS | candidate | L | G-OPS |
| 1366 | LIN-1894 | [D48](#d48) | Спроектировать backup verification command | OPS | candidate | L | G-OPS |
| 1367 | LIN-1895 | [D48](#d48) | Спроектировать restore rehearsal command | OPS | candidate | L | G-OPS |
| 1368 | LIN-1896 | [D48](#d48) | Спроектировать backup encryption boundary | OPS | candidate | L | G-OPS |
| 1369 | LIN-1897 | [D48](#d48) | Проверить restore wrong target refusal | CORRECTNESS | qualification | S | G-CORRECT |
| 1370 | LIN-1898 | [D48](#d48) | Проверить restore existing-data preservation | CORRECTNESS | qualification | S | G-CORRECT |
| 1371 | LIN-1899 | [D48](#d48) | Проверить restore partial-copy cleanup | CORRECTNESS | qualification | S | G-CORRECT |
| 1372 | LIN-1900 | [D48](#d48) | Проверить restore generation compatibility | CORRECTNESS | qualification | S | G-CORRECT |
| 1373 | LIN-1901 | [D48](#d48) | Добавить operator-safe maintenance mode | OPS | candidate | M | G-OPS |
| 1374 | LIN-1902 | [D48](#d48) | Добавить maintenance cancellation policy | OPS | candidate | M | G-OPS |
| 1375 | LIN-1903 | [D48](#d48) | Добавить background work budgets | OPS | candidate | M | G-OPS |
| 1376 | LIN-1904 | [D48](#d48) | Добавить maintenance progress snapshots | OPS | candidate | M | G-OPS |
| 1377 | LIN-1921 | [D49](#d49) | Проверить dependency feature minimality | CORRECTNESS | qualification | S | G-CORRECT |
| 1378 | LIN-1922 | [D49](#d49) | Проверить unused dependencies | CORRECTNESS | qualification | S | G-CORRECT |
| 1379 | LIN-1923 | [D49](#d49) | Проверить public dependency leakage | CORRECTNESS | qualification | S | G-CORRECT |
| 1380 | LIN-1924 | [D49](#d49) | Проверить dependency license inventory | CORRECTNESS | qualification | S | G-CORRECT |
| 1381 | LIN-1925 | [D49](#d49) | Измерить default build time | PERF | candidate | M | G-PERF |
| 1382 | LIN-1926 | [D49](#d49) | Измерить incremental build time | PERF | candidate | M | G-PERF |
| 1383 | LIN-1927 | [D49](#d49) | Измерить release link time | PERF | candidate | M | G-PERF |
| 1384 | LIN-1928 | [D49](#d49) | Измерить bundled DuckDB build footprint | PERF | candidate | M | G-PERF |
| 1385 | LIN-1929 | [D49](#d49) | Спроектировать optional peer benchmark features | OPS | candidate | L | G-OPS |
| 1386 | LIN-1930 | [D49](#d49) | Спроектировать lean CLI feature profile | OPS | candidate | L | G-OPS |
| 1387 | LIN-1931 | [D49](#d49) | Спроектировать embedded-only feature profile | OPS | candidate | L | G-OPS |
| 1388 | LIN-1932 | [D49](#d49) | Спроектировать compute-only feature profile | OPS | candidate | L | G-OPS |
| 1389 | LIN-1961 | [D50](#d50) | Опубликовать stable/experimental API matrix | DOC | candidate | S | G-DOC |
| 1390 | LIN-1962 | [D50](#d50) | Опубликовать feature support matrix | DOC | candidate | S | G-DOC |
| 1391 | LIN-1963 | [D50](#d50) | Опубликовать storage guarantee matrix | DOC | candidate | S | G-DOC |
| 1392 | LIN-1964 | [D50](#d50) | Опубликовать query semantics reference | DOC | candidate | S | G-DOC |
| 1393 | LIN-1965 | [D50](#d50) | Описать exact/approximate search distinction | DOC | candidate | S | G-DOC |
| 1394 | LIN-1966 | [D50](#d50) | Описать CPU/GPU precision differences | DOC | candidate | S | G-DOC |
| 1395 | LIN-1967 | [D50](#d50) | Описать NULL/absent-field distinction | DOC | candidate | S | G-DOC |
| 1396 | LIN-1968 | [D50](#d50) | Описать numeric equality boundaries | DOC | candidate | S | G-DOC |
| 1397 | LIN-1969 | [D50](#d50) | Описать WAL version compatibility | DOC | candidate | S | G-DOC |
| 1398 | LIN-1970 | [D50](#d50) | Описать snapshot version compatibility | DOC | candidate | S | G-DOC |
| 1399 | LIN-1971 | [D50](#d50) | Описать replication version compatibility | DOC | candidate | S | G-DOC |
| 1400 | LIN-1972 | [D50](#d50) | Описать downgrade support boundaries | DOC | candidate | S | G-DOC |

### P3: 600 пунктов

| Rank | ID | Область | Задача | Тип | Статус | Объём | Gate |
|---:|---|---|---|---|---|---|---|
| 1401 | LIN-0185 | [D05](#d05) | Проверить checkpoint WAL truncation race | CORRECTNESS | qualification | S | G-CORRECT |
| 1402 | LIN-0186 | [D05](#d05) | Проверить checkpoint follower pinning | CORRECTNESS | qualification | S | G-CORRECT |
| 1403 | LIN-0187 | [D05](#d05) | Проверить checkpoint reader pinning | CORRECTNESS | qualification | S | G-CORRECT |
| 1404 | LIN-0188 | [D05](#d05) | Проверить checkpoint cancellation cleanup | CORRECTNESS | qualification | S | G-CORRECT |
| 1405 | LIN-0189 | [D05](#d05) | Добавить checkpoint progress metrics | FEATURE | candidate | M | G-FEATURE |
| 1406 | LIN-0190 | [D05](#d05) | Добавить checkpoint debt metric | FEATURE | candidate | M | G-FEATURE |
| 1407 | LIN-0191 | [D05](#d05) | Добавить checkpoint size forecast | FEATURE | candidate | M | G-FEATURE |
| 1408 | LIN-0192 | [D05](#d05) | Добавить checkpoint error provenance | FEATURE | candidate | M | G-FEATURE |
| 1409 | LIN-0193 | [D05](#d05) | Сравнить cold snapshot formats | PERF | candidate | M | G-PERF |
| 1410 | LIN-0194 | [D05](#d05) | Сравнить snapshot encode chunk sizes | PERF | candidate | M | G-PERF |
| 1411 | LIN-0195 | [D05](#d05) | Сравнить mmap и buffered decode | PERF | done | M | G-PERF |
| 1412 | LIN-0196 | [D05](#d05) | Сравнить dictionary text snapshots | PERF | candidate | M | G-PERF |
| 1413 | LIN-0197 | [D05](#d05) | Проверить snapshot disk quota | CORRECTNESS | qualification | S | G-CORRECT |
| 1414 | LIN-0198 | [D05](#d05) | Проверить snapshot corruption isolation | CORRECTNESS | qualification | S | G-CORRECT |
| 1415 | LIN-0199 | [D05](#d05) | Проверить cross-version snapshot corpus | VERIFY | qualification | S | G-VERIFY |
| 1416 | LIN-0200 | [D05](#d05) | Проверить checkpoint restart idempotence | CORRECTNESS | qualification | S | G-CORRECT |
| 1417 | LIN-0345 | [D09](#d09) | Проверить clone_for_borrow contract | CORRECTNESS | qualification | S | G-CORRECT |
| 1418 | LIN-0346 | [D09](#d09) | Проверить clone_for_borrow SSO behavior | CORRECTNESS | qualification | S | G-CORRECT |
| 1419 | LIN-0347 | [D09](#d09) | Проверить cross-thread borrowed row ownership | CORRECTNESS | qualification | S | G-CORRECT |
| 1420 | LIN-0348 | [D09](#d09) | Проверить batch-to-row transfer ownership | CORRECTNESS | qualification | S | G-CORRECT |
| 1421 | LIN-0349 | [D09](#d09) | Ввести per-query allocation budget | PERF | candidate | M | G-PERF |
| 1422 | LIN-0350 | [D09](#d09) | Ввести per-plan retained-memory estimate | PERF | candidate | M | G-PERF |
| 1423 | LIN-0351 | [D09](#d09) | Ввести pool pressure telemetry | PERF | candidate | M | G-PERF |
| 1424 | LIN-0352 | [D09](#d09) | Ввести explicit pool purge API | PERF | candidate | M | G-PERF |
| 1425 | LIN-0353 | [D09](#d09) | Измерить allocator optional feature | PERF | candidate | M | G-PERF |
| 1426 | LIN-0354 | [D09](#d09) | Измерить allocator cold startup | PERF | candidate | M | G-PERF |
| 1427 | LIN-0355 | [D09](#d09) | Измерить allocator fragmentation churn | PERF | candidate | M | G-PERF |
| 1428 | LIN-0356 | [D09](#d09) | Измерить allocator multithread contention | PERF | candidate | M | G-PERF |
| 1429 | LIN-0357 | [D09](#d09) | Проверить OOM error boundaries | CORRECTNESS | qualification | S | G-CORRECT |
| 1430 | LIN-0358 | [D09](#d09) | Проверить capacity arithmetic saturation | CORRECTNESS | qualification | S | G-CORRECT |
| 1431 | LIN-0359 | [D09](#d09) | Проверить oversized reserve rejection | CORRECTNESS | qualification | S | G-CORRECT |
| 1432 | LIN-0360 | [D09](#d09) | Проверить memory release после cancel | CORRECTNESS | qualification | S | G-CORRECT |
| 1433 | LIN-0425 | [D11](#d11) | Проверить FK cycles policy | CORRECTNESS | qualification | S | G-CORRECT |
| 1434 | LIN-0426 | [D11](#d11) | Проверить dangling edge policy | CORRECTNESS | qualification | S | G-CORRECT |
| 1435 | LIN-0427 | [D11](#d11) | Проверить missing FK collection | CORRECTNESS | qualification | S | G-CORRECT |
| 1436 | LIN-0428 | [D11](#d11) | Проверить FK target schema changes | CORRECTNESS | qualification | S | G-CORRECT |
| 1437 | LIN-0429 | [D11](#d11) | Спроектировать constraint validation mode | FEATURE | candidate | L | G-FEATURE |
| 1438 | LIN-0430 | [D11](#d11) | Спроектировать migration constraint staging | FEATURE | candidate | L | G-FEATURE |
| 1439 | LIN-0431 | [D11](#d11) | Спроектировать constraint introspection | FEATURE | candidate | L | G-FEATURE |
| 1440 | LIN-0432 | [D11](#d11) | Спроектировать validation progress receipts | FEATURE | candidate | L | G-FEATURE |
| 1441 | LIN-0433 | [D11](#d11) | Ограничить complex constraint recursion | CORRECTNESS | qualification | M | G-CORRECT |
| 1442 | LIN-0434 | [D11](#d11) | Ограничить DNF constraint expansion | CORRECTNESS | qualification | M | G-CORRECT |
| 1443 | LIN-0435 | [D11](#d11) | Ограничить constraint diagnostic payload | CORRECTNESS | qualification | M | G-CORRECT |
| 1444 | LIN-0436 | [D11](#d11) | Ограничить batch conflict reporting | CORRECTNESS | qualification | M | G-CORRECT |
| 1445 | LIN-0437 | [D11](#d11) | Проверить exact numeric cross-backend parity | CORRECTNESS | qualification | S | G-CORRECT |
| 1446 | LIN-0438 | [D11](#d11) | Проверить unique constraint snapshot replay | CORRECTNESS | qualification | S | G-CORRECT |
| 1447 | LIN-0439 | [D11](#d11) | Проверить unique constraint WAL replay | CORRECTNESS | qualification | S | G-CORRECT |
| 1448 | LIN-0440 | [D11](#d11) | Проверить constraint disabled-feature errors | CORRECTNESS | qualification | S | G-CORRECT |
| 1449 | LIN-0465 | [D12](#d12) | Сократить temporary lexer strings | PERF | candidate | M | G-PERF |
| 1450 | LIN-0466 | [D12](#d12) | Измерить zero-copy token spans | PERF | candidate | M | G-PERF |
| 1451 | LIN-0467 | [D12](#d12) | Измерить keyword dispatch table | PERF | candidate | M | G-PERF |
| 1452 | LIN-0468 | [D12](#d12) | Измерить parser arena allocation | PERF | candidate | M | G-PERF |
| 1453 | LIN-0469 | [D12](#d12) | Добавить parse-only inspection API | API | candidate | M | G-API |
| 1454 | LIN-0470 | [D12](#d12) | Добавить canonical AST formatter | API | candidate | M | G-API |
| 1455 | LIN-0471 | [D12](#d12) | Добавить source-preserving query formatter | API | candidate | M | G-API |
| 1456 | LIN-0472 | [D12](#d12) | Добавить syntax version declaration | API | candidate | M | G-API |
| 1457 | LIN-0473 | [D12](#d12) | Проверить AST serde stability | CORRECTNESS | qualification | S | G-CORRECT |
| 1458 | LIN-0474 | [D12](#d12) | Проверить AST literal roundtrip | CORRECTNESS | qualification | S | G-CORRECT |
| 1459 | LIN-0475 | [D12](#d12) | Проверить AST constant fold provenance | CORRECTNESS | qualification | S | G-CORRECT |
| 1460 | LIN-0476 | [D12](#d12) | Проверить AST depth after expansion | CORRECTNESS | qualification | S | G-CORRECT |
| 1461 | LIN-0477 | [D12](#d12) | Подготовить parser fuzz target | VERIFY | qualification | S | G-VERIFY |
| 1462 | LIN-0478 | [D12](#d12) | Подготовить tokenizer corpus | VERIFY | qualification | S | G-VERIFY |
| 1463 | LIN-0479 | [D12](#d12) | Подготовить grammar conformance corpus | VERIFY | qualification | S | G-VERIFY |
| 1464 | LIN-0480 | [D12](#d12) | Подготовить malformed-query minimizer | API | candidate | M | G-API |
| 1465 | LIN-0585 | [D15](#d15) | Спроектировать calibrated CPU cost units | FEATURE | candidate | L | G-FEATURE |
| 1466 | LIN-0586 | [D15](#d15) | Спроектировать memory-pressure cost penalty | FEATURE | candidate | L | G-FEATURE |
| 1467 | LIN-0587 | [D15](#d15) | Спроектировать GPU transfer cost model | FEATURE | candidate | L | G-FEATURE |
| 1468 | LIN-0588 | [D15](#d15) | Спроектировать cold page cost model | FEATURE | candidate | L | G-FEATURE |
| 1469 | LIN-0589 | [D15](#d15) | Добавить EXPLAIN estimates/actual comparison | FEATURE | candidate | M | G-FEATURE |
| 1470 | LIN-0590 | [D15](#d15) | Добавить selectivity feedback receipts | FEATURE | candidate | M | G-FEATURE |
| 1471 | LIN-0591 | [D15](#d15) | Добавить estimator confidence fields | FEATURE | candidate | M | G-FEATURE |
| 1472 | LIN-0592 | [D15](#d15) | Добавить manual statistics inspection | FEATURE | candidate | M | G-FEATURE |
| 1473 | LIN-0593 | [D15](#d15) | Версионировать persisted statistics | FEATURE | candidate | M | G-FEATURE |
| 1474 | LIN-0594 | [D15](#d15) | Проверить statistics WAL lifecycle | CORRECTNESS | qualification | S | G-CORRECT |
| 1475 | LIN-0595 | [D15](#d15) | Проверить statistics snapshot lifecycle | CORRECTNESS | qualification | S | G-CORRECT |
| 1476 | LIN-0596 | [D15](#d15) | Проверить statistics schema migration | CORRECTNESS | qualification | S | G-CORRECT |
| 1477 | LIN-0597 | [D15](#d15) | Ограничить optimizer planning latency | CORRECTNESS | qualification | L | G-CORRECT |
| 1478 | LIN-0598 | [D15](#d15) | Ограничить statistics memory budget | CORRECTNESS | qualification | M | G-CORRECT |
| 1479 | LIN-0599 | [D15](#d15) | Добавить statistics reset operation | FEATURE | candidate | M | G-FEATURE |
| 1480 | LIN-0600 | [D15](#d15) | Добавить cost-model reproducibility fixtures | VERIFY | qualification | S | G-VERIFY |
| 1481 | LIN-0665 | [D17](#d17) | Добавить cursor consumed-row counters | API | candidate | M | G-API |
| 1482 | LIN-0666 | [D17](#d17) | Добавить cursor retained-byte counters | API | candidate | M | G-API |
| 1483 | LIN-0667 | [D17](#d17) | Добавить cursor cancellation receipt | API | candidate | M | G-API |
| 1484 | LIN-0668 | [D17](#d17) | Добавить cursor leak diagnostics | API | candidate | M | G-API |
| 1485 | LIN-0669 | [D17](#d17) | Проверить chained cursor error propagation | CORRECTNESS | qualification | S | G-CORRECT |
| 1486 | LIN-0670 | [D17](#d17) | Проверить cursor union deduplication | CORRECTNESS | qualification | S | G-CORRECT |
| 1487 | LIN-0671 | [D17](#d17) | Проверить cursor post-sort stability | CORRECTNESS | qualification | S | G-CORRECT |
| 1488 | LIN-0672 | [D17](#d17) | Проверить cursor graph traversal boundaries | CORRECTNESS | qualification | S | G-CORRECT |
| 1489 | LIN-0673 | [D17](#d17) | Измерить async stream chunk sizes | PERF | candidate | M | G-PERF |
| 1490 | LIN-0674 | [D17](#d17) | Измерить cursor to RecordBatch path | PERF | candidate | M | G-PERF |
| 1491 | LIN-0675 | [D17](#d17) | Измерить cursor iterator fusion | PERF | candidate | M | G-PERF |
| 1492 | LIN-0676 | [D17](#d17) | Измерить cursor field-slot cache | PERF | candidate | M | G-PERF |
| 1493 | LIN-0677 | [D17](#d17) | Проверить multi-cursor shared snapshots | CORRECTNESS | qualification | S | G-CORRECT |
| 1494 | LIN-0678 | [D17](#d17) | Проверить nested cursor reads | CORRECTNESS | qualification | S | G-CORRECT |
| 1495 | LIN-0679 | [D17](#d17) | Проверить cross-thread cursor restrictions | CORRECTNESS | qualification | S | G-CORRECT |
| 1496 | LIN-0680 | [D17](#d17) | Проверить cursor serialization rejection | CORRECTNESS | qualification | S | G-CORRECT |
| 1497 | LIN-0705 | [D18](#d18) | Проверить SmallText batch roundtrip | CORRECTNESS | qualification | S | G-CORRECT |
| 1498 | LIN-0706 | [D18](#d18) | Проверить Arc text batch roundtrip | CORRECTNESS | qualification | S | G-CORRECT |
| 1499 | LIN-0707 | [D18](#d18) | Проверить exact float bits batch | CORRECTNESS | qualification | S | G-CORRECT |
| 1500 | LIN-0708 | [D18](#d18) | Проверить timestamp batch roundtrip | CORRECTNESS | qualification | S | G-CORRECT |
| 1501 | LIN-0709 | [D18](#d18) | Спроектировать Arrow C data boundary | FEATURE | candidate | L | G-FEATURE |
| 1502 | LIN-0710 | [D18](#d18) | Спроектировать Arrow import ownership | FEATURE | candidate | L | G-FEATURE |
| 1503 | LIN-0711 | [D18](#d18) | Спроектировать Arrow export release callbacks | FEATURE | candidate | L | G-FEATURE |
| 1504 | LIN-0712 | [D18](#d18) | Спроектировать Arrow schema mapping | FEATURE | candidate | L | G-FEATURE |
| 1505 | LIN-0713 | [D18](#d18) | Добавить batch byte-size estimation | FEATURE | candidate | M | G-FEATURE |
| 1506 | LIN-0714 | [D18](#d18) | Добавить batch allocation limits | FEATURE | candidate | M | G-FEATURE |
| 1507 | LIN-0715 | [D18](#d18) | Добавить column memory diagnostics | FEATURE | candidate | M | G-FEATURE |
| 1508 | LIN-0716 | [D18](#d18) | Добавить batch consistency inspector | FEATURE | candidate | M | G-FEATURE |
| 1509 | LIN-0717 | [D18](#d18) | Проверить chunk boundary aggregation | CORRECTNESS | qualification | S | G-CORRECT |
| 1510 | LIN-0718 | [D18](#d18) | Проверить chunk boundary joins | CORRECTNESS | qualification | S | G-CORRECT |
| 1511 | LIN-0719 | [D18](#d18) | Проверить chunk boundary sorting | CORRECTNESS | qualification | S | G-CORRECT |
| 1512 | LIN-0720 | [D18](#d18) | Проверить chunk boundary cursor results | CORRECTNESS | qualification | S | G-CORRECT |
| 1513 | LIN-0785 | [D20](#d20) | Измерить indirect row-index sorting | PERF | candidate | M | G-PERF |
| 1514 | LIN-0786 | [D20](#d20) | Измерить inline sort keys | PERF | candidate | M | G-PERF |
| 1515 | LIN-0787 | [D20](#d20) | Измерить column sort permutation | PERF | candidate | M | G-PERF |
| 1516 | LIN-0788 | [D20](#d20) | Измерить sorting string dictionaries | PERF | candidate | M | G-PERF |
| 1517 | LIN-0789 | [D20](#d20) | Проверить CPU/GPU stable ordering | CORRECTNESS | qualification | S | G-CORRECT |
| 1518 | LIN-0790 | [D20](#d20) | Проверить GPU odd sort lengths | CORRECTNESS | qualification | S | G-CORRECT |
| 1519 | LIN-0791 | [D20](#d20) | Проверить GPU i64 rank extremes | CORRECTNESS | qualification | S | G-CORRECT |
| 1520 | LIN-0792 | [D20](#d20) | Проверить GPU sort allocation limits | CORRECTNESS | qualification | S | G-CORRECT |
| 1521 | LIN-0793 | [D20](#d20) | Добавить sort memory estimates | FEATURE | candidate | M | G-FEATURE |
| 1522 | LIN-0794 | [D20](#d20) | Добавить comparator evaluation counters | FEATURE | candidate | M | G-FEATURE |
| 1523 | LIN-0795 | [D20](#d20) | Добавить Top-K rejection counters | FEATURE | candidate | M | G-FEATURE |
| 1524 | LIN-0796 | [D20](#d20) | Добавить spill-size diagnostics | FEATURE | candidate | L | G-FEATURE |
| 1525 | LIN-0797 | [D20](#d20) | Сравнить ordered-index scan с sort | PERF | candidate | M | G-PERF |
| 1526 | LIN-0798 | [D20](#d20) | Сравнить prefix-sorted execution | PERF | candidate | M | G-PERF |
| 1527 | LIN-0799 | [D20](#d20) | Сравнить radix sort для integers | PERF | candidate | M | G-PERF |
| 1528 | LIN-0800 | [D20](#d20) | Сравнить adaptive small-array sorting | PERF | candidate | M | G-PERF |
| 1529 | LIN-0825 | [D21](#d21) | Измерить selective-side prefilter | PERF | candidate | M | G-PERF |
| 1530 | LIN-0826 | [D21](#d21) | Измерить bitmap semi-join filter | PERF | candidate | M | G-PERF |
| 1531 | LIN-0827 | [D21](#d21) | Измерить bloom join prefilter | PERF | candidate | M | G-PERF |
| 1532 | LIN-0828 | [D21](#d21) | Измерить adaptive join crossover | PERF | candidate | M | G-PERF |
| 1533 | LIN-0829 | [D21](#d21) | Проверить join cancellation boundary | CORRECTNESS | qualification | S | G-CORRECT |
| 1534 | LIN-0830 | [D21](#d21) | Проверить join error atomicity | CORRECTNESS | qualification | S | G-CORRECT |
| 1535 | LIN-0831 | [D21](#d21) | Проверить cursor join first-row latency | CORRECTNESS | qualification | S | G-CORRECT |
| 1536 | LIN-0832 | [D21](#d21) | Проверить projected join field pruning | CORRECTNESS | qualification | S | G-CORRECT |
| 1537 | LIN-0833 | [D21](#d21) | Проверить CPU/GPU join multiplicity | CORRECTNESS | qualification | S | G-CORRECT |
| 1538 | LIN-0834 | [D21](#d21) | Проверить CPU/GPU join identity order | CORRECTNESS | qualification | S | G-CORRECT |
| 1539 | LIN-0835 | [D21](#d21) | Проверить tiled join boundary sizes | CORRECTNESS | qualification | S | G-CORRECT |
| 1540 | LIN-0836 | [D21](#d21) | Проверить GPU join no-match cases | VERIFY | qualification | S | G-VERIFY |
| 1541 | LIN-0837 | [D21](#d21) | Добавить join estimates/actual metrics | FEATURE | candidate | M | G-FEATURE |
| 1542 | LIN-0838 | [D21](#d21) | Добавить hot-key diagnostics | FEATURE | candidate | M | G-FEATURE |
| 1543 | LIN-0839 | [D21](#d21) | Добавить build/probe timing split | FEATURE | candidate | M | G-FEATURE |
| 1544 | LIN-0840 | [D21](#d21) | Добавить join algorithm explain fields | FEATURE | candidate | M | G-FEATURE |
| 1545 | LIN-0865 | [D22](#d22) | Измерить reusable partition buffers | PERF | candidate | M | G-PERF |
| 1546 | LIN-0866 | [D22](#d22) | Измерить sorted-partition reuse | PERF | candidate | M | G-PERF |
| 1547 | LIN-0867 | [D22](#d22) | Измерить multiple-window sharing | PERF | candidate | M | G-PERF |
| 1548 | LIN-0868 | [D22](#d22) | Измерить monotonic deque min/max | PERF | candidate | M | G-PERF |
| 1549 | LIN-0869 | [D22](#d22) | Спроектировать bounded streaming windows | FEATURE | candidate | L | G-FEATURE |
| 1550 | LIN-0870 | [D22](#d22) | Спроектировать spillable partitions | FEATURE | candidate | L | G-FEATURE |
| 1551 | LIN-0871 | [D22](#d22) | Спроектировать window cancellation checkpoints | FEATURE | candidate | L | G-FEATURE |
| 1552 | LIN-0872 | [D22](#d22) | Спроектировать window memory estimation | FEATURE | candidate | L | G-FEATURE |
| 1553 | LIN-0873 | [D22](#d22) | Проверить window после join | CORRECTNESS | qualification | S | G-CORRECT |
| 1554 | LIN-0874 | [D22](#d22) | Проверить window до limit | CORRECTNESS | qualification | S | G-CORRECT |
| 1555 | LIN-0875 | [D22](#d22) | Проверить window projection aliases | CORRECTNESS | qualification | S | G-CORRECT |
| 1556 | LIN-0876 | [D22](#d22) | Проверить nested window rejection | CORRECTNESS | qualification | S | G-CORRECT |
| 1557 | LIN-0877 | [D22](#d22) | Проверить partition size diagnostics | CORRECTNESS | qualification | S | G-CORRECT |
| 1558 | LIN-0878 | [D22](#d22) | Проверить frame evaluation counters | CORRECTNESS | qualification | S | G-CORRECT |
| 1559 | LIN-0879 | [D22](#d22) | Проверить window reference oracle | VERIFY | qualification | S | G-VERIFY |
| 1560 | LIN-0880 | [D22](#d22) | Проверить window differential fixtures | VERIFY | qualification | S | G-VERIFY |
| 1561 | LIN-0945 | [D24](#d24) | Измерить bounded candidate heaps | PERF | candidate | M | G-PERF |
| 1562 | LIN-0946 | [D24](#d24) | Измерить score-only cursor path | PERF | candidate | M | G-PERF |
| 1563 | LIN-0947 | [D24](#d24) | Измерить query-time allocation count | PERF | candidate | M | G-PERF |
| 1564 | LIN-0948 | [D24](#d24) | Измерить post-filter versus prefilter | PERF | candidate | M | G-PERF |
| 1565 | LIN-0949 | [D24](#d24) | Добавить snippet highlight offsets | FEATURE | candidate | M | G-FEATURE |
| 1566 | LIN-0950 | [D24](#d24) | Добавить matched-term explanations | FEATURE | candidate | M | G-FEATURE |
| 1567 | LIN-0951 | [D24](#d24) | Добавить hit-count estimation policy | FEATURE | candidate | M | G-FEATURE |
| 1568 | LIN-0952 | [D24](#d24) | Добавить FTS query complexity budget | FEATURE | candidate | M | G-FEATURE |
| 1569 | LIN-0953 | [D24](#d24) | Проверить query cancellation внутри postings | CORRECTNESS | qualification | S | G-CORRECT |
| 1570 | LIN-0954 | [D24](#d24) | Проверить pending-delete candidate filtering | CORRECTNESS | qualification | S | G-CORRECT |
| 1571 | LIN-0955 | [D24](#d24) | Проверить mutation-generation query visibility | CORRECTNESS | qualification | S | G-CORRECT |
| 1572 | LIN-0956 | [D24](#d24) | Проверить rebuilt/live FTS score parity | CORRECTNESS | qualification | S | G-CORRECT |
| 1573 | LIN-0957 | [D24](#d24) | Добавить query term fanout metrics | FEATURE | candidate | M | G-FEATURE |
| 1574 | LIN-0958 | [D24](#d24) | Добавить candidate pruning counters | FEATURE | candidate | M | G-FEATURE |
| 1575 | LIN-0959 | [D24](#d24) | Добавить score distribution diagnostics | FEATURE | candidate | M | G-FEATURE |
| 1576 | LIN-0960 | [D24](#d24) | Добавить FTS slow-query fixtures | VERIFY | qualification | S | G-VERIFY |
| 1577 | LIN-1025 | [D26](#d26) | Спроектировать deterministic model revision | FEATURE | candidate | L | G-FEATURE |
| 1578 | LIN-1026 | [D26](#d26) | Спроектировать embedding recompute command | FEATURE | candidate | L | G-FEATURE |
| 1579 | LIN-1027 | [D26](#d26) | Спроектировать selective reembedding policy | FEATURE | candidate | L | G-FEATURE |
| 1580 | LIN-1028 | [D26](#d26) | Спроектировать stale embedding reporting | FEATURE | candidate | L | G-FEATURE |
| 1581 | LIN-1029 | [D26](#d26) | Проверить malformed custom batch output | CORRECTNESS | qualification | S | G-CORRECT |
| 1582 | LIN-1030 | [D26](#d26) | Проверить custom embedder panic boundary | CORRECTNESS | qualification | S | G-CORRECT |
| 1583 | LIN-1031 | [D26](#d26) | Проверить batch output dimension mismatch | CORRECTNESS | qualification | S | G-CORRECT |
| 1584 | LIN-1032 | [D26](#d26) | Проверить custom embedder ordering | CORRECTNESS | qualification | S | G-CORRECT |
| 1585 | LIN-1033 | [D26](#d26) | Добавить embed text inspection API | FEATURE | candidate | M | G-FEATURE |
| 1586 | LIN-1034 | [D26](#d26) | Добавить per-model latency metrics | FEATURE | candidate | M | G-FEATURE |
| 1587 | LIN-1035 | [D26](#d26) | Добавить embedded token count metrics | FEATURE | candidate | M | G-FEATURE |
| 1588 | LIN-1036 | [D26](#d26) | Добавить model memory accounting | FEATURE | candidate | M | G-FEATURE |
| 1589 | LIN-1037 | [D26](#d26) | Ограничить embedding text bytes | CORRECTNESS | qualification | M | G-CORRECT |
| 1590 | LIN-1038 | [D26](#d26) | Ограничить embedding batch output bytes | CORRECTNESS | qualification | M | G-CORRECT |
| 1591 | LIN-1039 | [D26](#d26) | Ограничить model initialization latency | CORRECTNESS | qualification | M | G-CORRECT |
| 1592 | LIN-1040 | [D26](#d26) | Ограничить reembedding transaction size | CORRECTNESS | qualification | M | G-CORRECT |
| 1593 | LIN-1065 | [D27](#d27) | Спроектировать background ANN rebuild | FEATURE | candidate | L | G-FEATURE |
| 1594 | LIN-1066 | [D27](#d27) | Спроектировать bounded ANN rebuild memory | FEATURE | candidate | L | G-FEATURE |
| 1595 | LIN-1067 | [D27](#d27) | Спроектировать ANN generation swaps | FEATURE | candidate | L | G-FEATURE |
| 1596 | LIN-1068 | [D27](#d27) | Спроектировать ANN persistence version | FEATURE | candidate | L | G-FEATURE |
| 1597 | LIN-1069 | [D27](#d27) | Проверить ANN concurrent snapshot reads | CORRECTNESS | qualification | L | G-CORRECT |
| 1598 | LIN-1070 | [D27](#d27) | Проверить ANN rollback visibility | CORRECTNESS | qualification | L | G-CORRECT |
| 1599 | LIN-1071 | [D27](#d27) | Проверить ANN checkpoint reopening | CORRECTNESS | qualification | L | G-CORRECT |
| 1600 | LIN-1072 | [D27](#d27) | Проверить ANN rebuild cancellation | CORRECTNESS | qualification | L | G-CORRECT |
| 1601 | LIN-1073 | [D27](#d27) | Добавить recall regression fixtures | VERIFY | qualification | S | G-VERIFY |
| 1602 | LIN-1074 | [D27](#d27) | Добавить ANN traversal counters | FEATURE | candidate | L | G-FEATURE |
| 1603 | LIN-1075 | [D27](#d27) | Добавить graph disconnectedness diagnostics | FEATURE | candidate | M | G-FEATURE |
| 1604 | LIN-1076 | [D27](#d27) | Добавить search expansion budget | FEATURE | candidate | M | G-FEATURE |
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
| 1616 | LIN-1112 | [D28](#d28) | Проверить neural model identity mismatch | CORRECTNESS | qualification | S | G-CORRECT |
| 1617 | LIN-1113 | [D28](#d28) | Ограничить query vector dimensions | CORRECTNESS | qualification | M | G-CORRECT |
| 1618 | LIN-1114 | [D28](#d28) | Ограничить candidate reranking bytes | CORRECTNESS | qualification | M | G-CORRECT |
| 1619 | LIN-1115 | [D28](#d28) | Ограничить neural embedding concurrency | CORRECTNESS | qualification | M | G-CORRECT |
| 1620 | LIN-1116 | [D28](#d28) | Ограничить embedding RPC timeout | CORRECTNESS | qualification | M | G-CORRECT |
| 1621 | LIN-1117 | [D28](#d28) | Добавить exact-search reference fixtures | VERIFY | qualification | S | G-VERIFY |
| 1622 | LIN-1118 | [D28](#d28) | Добавить near-tie score corpus | VERIFY | qualification | S | G-VERIFY |
| 1623 | LIN-1119 | [D28](#d28) | Добавить cross-platform vector golden tests | FEATURE | candidate | M | G-FEATURE |
| 1624 | LIN-1120 | [D28](#d28) | Добавить vector cache invalidation diagnostics | FEATURE | candidate | M | G-FEATURE |
| 1625 | LIN-1145 | [D29](#d29) | Проверить checked-scope nesting rejection | CORRECTNESS | qualification | S | G-CORRECT |
| 1626 | LIN-1146 | [D29](#d29) | Проверить invalid shader recovery | CORRECTNESS | qualification | S | G-CORRECT |
| 1627 | LIN-1147 | [D29](#d29) | Проверить invalid binding recovery | CORRECTNESS | qualification | S | G-CORRECT |
| 1628 | LIN-1148 | [D29](#d29) | Проверить invalid dispatch recovery | CORRECTNESS | qualification | S | G-CORRECT |
| 1629 | LIN-1149 | [D29](#d29) | Добавить adapter capability report | FEATURE | done | M | G-FEATURE |
| 1630 | LIN-1150 | [D29](#d29) | Добавить resident allocation counters | FEATURE | done | M | G-FEATURE |
| 1631 | LIN-1151 | [D29](#d29) | Добавить transfer byte counters | FEATURE | done | M | G-FEATURE |
| 1632 | LIN-1152 | [D29](#d29) | Добавить device error provenance | FEATURE | candidate | M | G-FEATURE |
| 1633 | LIN-1153 | [D29](#d29) | Проверить Metal backend conformance | CORRECTNESS | qualification | S | G-CORRECT |
| 1634 | LIN-1154 | [D29](#d29) | Проверить Vulkan backend conformance | CORRECTNESS | qualification | S | G-CORRECT |
| 1635 | LIN-1155 | [D29](#d29) | Проверить DX12 backend conformance | CORRECTNESS | qualification | S | G-CORRECT |
| 1636 | LIN-1156 | [D29](#d29) | Проверить software-adapter labeling | CORRECTNESS | qualification | S | G-CORRECT |
| 1637 | LIN-1157 | [D29](#d29) | Спроектировать GPU admission control | FEATURE | candidate | L | G-FEATURE |
| 1638 | LIN-1158 | [D29](#d29) | Спроектировать fair multi-client queue | FEATURE | candidate | L | G-FEATURE |
| 1639 | LIN-1159 | [D29](#d29) | Спроектировать query cancellation fences | FEATURE | candidate | L | G-FEATURE |
| 1640 | LIN-1160 | [D29](#d29) | Спроектировать graceful unavailable-device errors | FEATURE | candidate | L | G-FEATURE |
| 1641 | LIN-1185 | [D30](#d30) | Спроектировать GPU execution capability matrix | FEATURE | candidate | L | G-FEATURE |
| 1642 | LIN-1186 | [D30](#d30) | Спроектировать explicit numerical tolerance policy | FEATURE | candidate | L | G-FEATURE |
| 1643 | LIN-1187 | [D30](#d30) | Спроектировать exact-kernel fallback rejection | FEATURE | candidate | L | G-FEATURE |
| 1644 | LIN-1188 | [D30](#d30) | Спроектировать kernel ABI versioning | FEATURE | candidate | L | G-FEATURE |
| 1645 | LIN-1189 | [D30](#d30) | Добавить randomized CPU/GPU differential tests | FEATURE | candidate | M | G-FEATURE |
| 1646 | LIN-1190 | [D30](#d30) | Добавить backend-specific shader corpus | VERIFY | qualification | S | G-VERIFY |
| 1647 | LIN-1191 | [D30](#d30) | Добавить shader complexity budgets | FEATURE | candidate | M | G-FEATURE |
| 1648 | LIN-1192 | [D30](#d30) | Добавить shader validation artifact hashes | FEATURE | candidate | M | G-FEATURE |
| 1649 | LIN-1193 | [D30](#d30) | Измерить small dispatch batching | PERF | candidate | M | G-PERF |
| 1650 | LIN-1194 | [D30](#d30) | Измерить workgroup size tuning | PERF | candidate | M | G-PERF |
| 1651 | LIN-1195 | [D30](#d30) | Измерить alignment-aware packed columns | PERF | candidate | M | G-PERF |
| 1652 | LIN-1196 | [D30](#d30) | Измерить transfer-free scalar result path | PERF | candidate | M | G-PERF |
| 1653 | LIN-1197 | [D30](#d30) | Проверить partial final workgroups | CORRECTNESS | qualification | S | G-CORRECT |
| 1654 | LIN-1198 | [D30](#d30) | Проверить zero-size dispatch behavior | CORRECTNESS | qualification | S | G-CORRECT |
| 1655 | LIN-1199 | [D30](#d30) | Проверить one-element kernel behavior | CORRECTNESS | qualification | S | G-CORRECT |
| 1656 | LIN-1200 | [D30](#d30) | Проверить maximum supported dispatch boundaries | CORRECTNESS | qualification | S | G-CORRECT |
| 1657 | LIN-1225 | [D31](#d31) | Проверить graph mutation rollback | CORRECTNESS | qualification | S | G-CORRECT |
| 1658 | LIN-1226 | [D31](#d31) | Проверить graph replay consistency | CORRECTNESS | qualification | S | G-CORRECT |
| 1659 | LIN-1227 | [D31](#d31) | Проверить graph snapshot consistency | CORRECTNESS | qualification | S | G-CORRECT |
| 1660 | LIN-1228 | [D31](#d31) | Проверить graph index rebuild consistency | CORRECTNESS | qualification | S | G-CORRECT |
| 1661 | LIN-1229 | [D31](#d31) | Спроектировать graph snapshot cursors | FEATURE | candidate | L | G-FEATURE |
| 1662 | LIN-1230 | [D31](#d31) | Спроектировать streaming path results | FEATURE | candidate | L | G-FEATURE |
| 1663 | LIN-1231 | [D31](#d31) | Спроектировать traversal cancellation points | FEATURE | candidate | L | G-FEATURE |
| 1664 | LIN-1232 | [D31](#d31) | Спроектировать traversal continuation tokens | FEATURE | candidate | L | G-FEATURE |
| 1665 | LIN-1233 | [D31](#d31) | Проверить CPU/GPU traversal identity sets | CORRECTNESS | qualification | S | G-CORRECT |
| 1666 | LIN-1234 | [D31](#d31) | Проверить CPU/GPU traversal ordering | CORRECTNESS | qualification | S | G-CORRECT |
| 1667 | LIN-1235 | [D31](#d31) | Проверить GPU path cap boundary | CORRECTNESS | qualification | S | G-CORRECT |
| 1668 | LIN-1236 | [D31](#d31) | Проверить mixed collection graph targets | CORRECTNESS | qualification | S | G-CORRECT |
| 1669 | LIN-1237 | [D31](#d31) | Добавить degree distribution statistics | FEATURE | candidate | M | G-FEATURE |
| 1670 | LIN-1238 | [D31](#d31) | Добавить graph consistency checker | FEATURE | candidate | M | G-FEATURE |
| 1671 | LIN-1239 | [D31](#d31) | Добавить traversal expansion metrics | FEATURE | candidate | M | G-FEATURE |
| 1672 | LIN-1240 | [D31](#d31) | Добавить adversarial graph fixtures | VERIFY | qualification | S | G-VERIFY |
| 1673 | LIN-1265 | [D32](#d32) | Добавить cold residency diagnostics | FEATURE | candidate | M | G-FEATURE |
| 1674 | LIN-1266 | [D32](#d32) | Добавить page-fault phase metrics | FEATURE | candidate | M | G-FEATURE |
| 1675 | LIN-1267 | [D32](#d32) | Добавить cold working-set estimates | FEATURE | candidate | M | G-FEATURE |
| 1676 | LIN-1268 | [D32](#d32) | Добавить cache eviction reasons | FEATURE | candidate | M | G-FEATURE |
| 1677 | LIN-1269 | [D32](#d32) | Проверить cold-to-hot mutation semantics | CORRECTNESS | qualification | S | G-CORRECT |
| 1678 | LIN-1270 | [D32](#d32) | Проверить cold collection deletion | CORRECTNESS | qualification | S | G-CORRECT |
| 1679 | LIN-1271 | [D32](#d32) | Проверить cold tier snapshot copying | CORRECTNESS | qualification | S | G-CORRECT |
| 1680 | LIN-1272 | [D32](#d32) | Проверить cold checkpoint handoff | CORRECTNESS | qualification | S | G-CORRECT |
| 1681 | LIN-1273 | [D32](#d32) | Спроектировать explicit warmup API | FEATURE | candidate | L | G-FEATURE |
| 1682 | LIN-1274 | [D32](#d32) | Спроектировать bounded warmup operation | FEATURE | candidate | L | G-FEATURE |
| 1683 | LIN-1275 | [D32](#d32) | Спроектировать cache purge inspection | FEATURE | candidate | L | G-FEATURE |
| 1684 | LIN-1276 | [D32](#d32) | Спроектировать cold retention policy | FEATURE | candidate | L | G-FEATURE |
| 1685 | LIN-1277 | [D32](#d32) | Проверить Linux mmap behavior | CORRECTNESS | qualification | S | G-CORRECT |
| 1686 | LIN-1278 | [D32](#d32) | Проверить Darwin mmap behavior | CORRECTNESS | qualification | S | G-CORRECT |
| 1687 | LIN-1279 | [D32](#d32) | Проверить Windows mapped-file behavior | CORRECTNESS | qualification | S | G-CORRECT |
| 1688 | LIN-1280 | [D32](#d32) | Проверить network-backed cold policy | CORRECTNESS | qualification | S | G-CORRECT |
| 1689 | LIN-1305 | [D33](#d33) | Проверить multiple simultaneous streams | CORRECTNESS | qualification | S | G-CORRECT |
| 1690 | LIN-1306 | [D33](#d33) | Проверить async read-after-write | CORRECTNESS | qualification | S | G-CORRECT |
| 1691 | LIN-1307 | [D33](#d33) | Проверить async query panic propagation | CORRECTNESS | qualification | S | G-CORRECT |
| 1692 | LIN-1308 | [D33](#d33) | Проверить blocking pool shutdown | CORRECTNESS | qualification | S | G-CORRECT |
| 1693 | LIN-1309 | [D33](#d33) | Спроектировать bounded embedding requests | API | candidate | L | G-API |
| 1694 | LIN-1310 | [D33](#d33) | Спроектировать async replica reads | API | candidate | L | G-API |
| 1695 | LIN-1311 | [D33](#d33) | Спроектировать batched async writes | API | candidate | L | G-API |
| 1696 | LIN-1312 | [D33](#d33) | Спроектировать async checkpoint API | API | candidate | L | G-API |
| 1697 | LIN-1313 | [D33](#d33) | Проверить async Send/Sync guarantees | CORRECTNESS | qualification | S | G-CORRECT |
| 1698 | LIN-1314 | [D33](#d33) | Проверить borrowed input lifetime restrictions | CORRECTNESS | qualification | S | G-CORRECT |
| 1699 | LIN-1315 | [D33](#d33) | Проверить runtime-independent adapter boundaries | CORRECTNESS | qualification | S | G-CORRECT |
| 1700 | LIN-1316 | [D33](#d33) | Проверить disabled-async build behavior | CORRECTNESS | qualification | S | G-CORRECT |
| 1701 | LIN-1317 | [D33](#d33) | Добавить adversarial slow-consumer tests | API | candidate | M | G-API |
| 1702 | LIN-1318 | [D33](#d33) | Добавить cancellation race corpus | VERIFY | qualification | S | G-VERIFY |
| 1703 | LIN-1319 | [D33](#d33) | Добавить queue saturation benchmarks | API | candidate | M | G-API |
| 1704 | LIN-1320 | [D33](#d33) | Добавить graceful-shutdown integration cases | VERIFY | qualification | S | G-VERIFY |
| 1705 | LIN-1345 | [D34](#d34) | Спроектировать bounded snapshot history | FEATURE | candidate | L | G-FEATURE |
| 1706 | LIN-1346 | [D34](#d34) | Спроектировать read transaction handles | FEATURE | candidate | L | G-FEATURE |
| 1707 | LIN-1347 | [D34](#d34) | Спроектировать snapshot export token | FEATURE | candidate | L | G-FEATURE |
| 1708 | LIN-1348 | [D34](#d34) | Спроектировать repeatable-read query groups | FEATURE | candidate | L | G-FEATURE |
| 1709 | LIN-1349 | [D34](#d34) | Проверить lock acquisition ordering | CORRECTNESS | qualification | S | G-CORRECT |
| 1710 | LIN-1350 | [D34](#d34) | Проверить poisoned lock handling | CORRECTNESS | qualification | S | G-CORRECT |
| 1711 | LIN-1351 | [D34](#d34) | Проверить concurrent cache invalidation | CORRECTNESS | qualification | S | G-CORRECT |
| 1712 | LIN-1352 | [D34](#d34) | Проверить shared model initialization | CORRECTNESS | qualification | S | G-CORRECT |
| 1713 | LIN-1353 | [D34](#d34) | Сравнить RwLock snapshot publication | PERF | candidate | M | G-PERF |
| 1714 | LIN-1354 | [D34](#d34) | Сравнить atomic Arc publication | PERF | candidate | M | G-PERF |
| 1715 | LIN-1355 | [D34](#d34) | Сравнить copy-on-write metadata | PERF | candidate | M | G-PERF |
| 1716 | LIN-1356 | [D34](#d34) | Сравнить per-query borrowed metadata | PERF | candidate | M | G-PERF |
| 1717 | LIN-1357 | [D34](#d34) | Добавить loom publication models | FEATURE | candidate | M | G-FEATURE |
| 1718 | LIN-1358 | [D34](#d34) | Добавить deterministic concurrency harness | FEATURE | candidate | M | G-FEATURE |
| 1719 | LIN-1359 | [D34](#d34) | Добавить linearizability history checks | FEATURE | candidate | M | G-FEATURE |
| 1720 | LIN-1360 | [D34](#d34) | Добавить contention stress fixtures | VERIFY | qualification | S | G-VERIFY |
| 1721 | LIN-1385 | [D35](#d35) | Добавить replication lag time metric | FEATURE | candidate | M | G-FEATURE |
| 1722 | LIN-1386 | [D35](#d35) | Добавить replication lag bytes metric | FEATURE | candidate | M | G-FEATURE |
| 1723 | LIN-1387 | [D35](#d35) | Добавить handshake failure categories | FEATURE | candidate | M | G-FEATURE |
| 1724 | LIN-1388 | [D35](#d35) | Добавить reconnect attempt counters | FEATURE | candidate | M | G-FEATURE |
| 1725 | LIN-1389 | [D35](#d35) | Спроектировать compressed transport negotiation | FEATURE | candidate | L | G-FEATURE |
| 1726 | LIN-1390 | [D35](#d35) | Спроектировать streamed backup transfer | FEATURE | candidate | L | G-FEATURE |
| 1727 | LIN-1391 | [D35](#d35) | Спроектировать replication resumable offsets | FEATURE | candidate | L | G-FEATURE |
| 1728 | LIN-1392 | [D35](#d35) | Спроектировать transport capability negotiation | FEATURE | candidate | L | G-FEATURE |
| 1729 | LIN-1393 | [D35](#d35) | Проверить cancellation при transfer | CORRECTNESS | qualification | S | G-CORRECT |
| 1730 | LIN-1394 | [D35](#d35) | Проверить slow follower backpressure | CORRECTNESS | qualification | S | G-CORRECT |
| 1731 | LIN-1395 | [D35](#d35) | Проверить abusive sender resource bound | CORRECTNESS | qualification | S | G-CORRECT |
| 1732 | LIN-1396 | [D35](#d35) | Проверить graceful server shutdown | CORRECTNESS | qualification | S | G-CORRECT |
| 1733 | LIN-1397 | [D35](#d35) | Добавить network fault replay corpus | VERIFY | qualification | S | G-VERIFY |
| 1734 | LIN-1398 | [D35](#d35) | Добавить TLS integration matrix | FEATURE | candidate | M | G-FEATURE |
| 1735 | LIN-1399 | [D35](#d35) | Добавить transport version compatibility fixtures | VERIFY | qualification | S | G-VERIFY |
| 1736 | LIN-1400 | [D35](#d35) | Добавить loopback latency benchmarks | FEATURE | candidate | M | G-FEATURE |
| 1737 | LIN-1425 | [D36](#d36) | Проверить promoted node generation identity | CORRECTNESS | qualification | S | G-CORRECT |
| 1738 | LIN-1426 | [D36](#d36) | Проверить split-brain prevention contract | CORRECTNESS | qualification | S | G-CORRECT |
| 1739 | LIN-1427 | [D36](#d36) | Проверить old-primary write rejection | CORRECTNESS | qualification | S | G-CORRECT |
| 1740 | LIN-1428 | [D36](#d36) | Проверить resync после divergent history | CORRECTNESS | qualification | S | G-CORRECT |
| 1741 | LIN-1429 | [D36](#d36) | Добавить follower health inspection | FEATURE | candidate | M | G-FEATURE |
| 1742 | LIN-1430 | [D36](#d36) | Добавить catch-up ETA uncertainty | FEATURE | candidate | M | G-FEATURE |
| 1743 | LIN-1431 | [D36](#d36) | Добавить retention pressure metrics | FEATURE | candidate | M | G-FEATURE |
| 1744 | LIN-1432 | [D36](#d36) | Добавить failed-generation diagnostics | FEATURE | candidate | M | G-FEATURE |
| 1745 | LIN-1433 | [D36](#d36) | Измерить live-query catch-up impact | PERF | candidate | M | G-PERF |
| 1746 | LIN-1434 | [D36](#d36) | Измерить replay/index pipeline crossover | PERF | candidate | M | G-PERF |
| 1747 | LIN-1435 | [D36](#d36) | Измерить batched follower fsync | PERF | candidate | M | G-PERF |
| 1748 | LIN-1436 | [D36](#d36) | Измерить initial bootstrap RSS | PERF | candidate | M | G-PERF |
| 1749 | LIN-1437 | [D36](#d36) | Добавить follower chaos scenarios | FEATURE | candidate | M | G-FEATURE |
| 1750 | LIN-1438 | [D36](#d36) | Добавить multi-follower integration tests | VERIFY | qualification | S | G-VERIFY |
| 1751 | LIN-1439 | [D36](#d36) | Добавить reconnect idempotence corpus | VERIFY | qualification | S | G-VERIFY |
| 1752 | LIN-1440 | [D36](#d36) | Добавить promotion recovery playbook | FEATURE | candidate | M | G-FEATURE |
| 1753 | LIN-1465 | [D37](#d37) | Проверить query reentrancy rules | CORRECTNESS | qualification | S | G-CORRECT |
| 1754 | LIN-1466 | [D37](#d37) | Проверить async wrapper parity | CORRECTNESS | qualification | S | G-CORRECT |
| 1755 | LIN-1467 | [D37](#d37) | Проверить ReadDb handle ergonomics | CORRECTNESS | qualification | S | G-CORRECT |
| 1756 | LIN-1468 | [D37](#d37) | Проверить external embedder ownership | CORRECTNESS | qualification | S | G-CORRECT |
| 1757 | LIN-1469 | [D37](#d37) | Разделить stable и experimental exports | API | candidate | M | G-API |
| 1758 | LIN-1470 | [D37](#d37) | Разделить internal/public storage types | API | candidate | M | G-API |
| 1759 | LIN-1471 | [D37](#d37) | Разделить unsafe extension boundaries | API | candidate | M | G-API |
| 1760 | LIN-1472 | [D37](#d37) | Разделить backend-specific option types | API | candidate | M | G-API |
| 1761 | LIN-1473 | [D37](#d37) | Добавить semver compile-test suite | API | candidate | M | G-API |
| 1762 | LIN-1474 | [D37](#d37) | Добавить downstream allocator coexistence test | API | candidate | M | G-API |
| 1763 | LIN-1475 | [D37](#d37) | Добавить public API size regression check | API | candidate | M | G-API |
| 1764 | LIN-1476 | [D37](#d37) | Добавить rustdoc runnable examples | API | candidate | M | G-API |
| 1765 | LIN-1477 | [D37](#d37) | Спроектировать C ABI feasibility boundary | API | candidate | L | G-API |
| 1766 | LIN-1478 | [D37](#d37) | Спроектировать Python binding ownership | API | candidate | L | G-API |
| 1767 | LIN-1479 | [D37](#d37) | Спроектировать WASM storage limitations | API | candidate | L | G-API |
| 1768 | LIN-1480 | [D37](#d37) | Спроектировать extension provider lifecycle | API | candidate | L | G-API |
| 1769 | LIN-1505 | [D38](#d38) | Спроектировать transparent newtype mapping | API | candidate | L | G-API |
| 1770 | LIN-1506 | [D38](#d38) | Спроектировать enum scalar mapping | API | candidate | L | G-API |
| 1771 | LIN-1507 | [D38](#d38) | Спроектировать decimal mapping policy | API | candidate | L | G-API |
| 1772 | LIN-1508 | [D38](#d38) | Спроектировать UUID mapping policy | API | candidate | L | G-API |
| 1773 | LIN-1509 | [D38](#d38) | Добавить compile-fail diagnostic snapshots | VERIFY | qualification | S | G-VERIFY |
| 1774 | LIN-1510 | [D38](#d38) | Добавить downstream integration examples | API | candidate | M | G-API |
| 1775 | LIN-1511 | [D38](#d38) | Добавить derive MSRV checks | API | candidate | M | G-API |
| 1776 | LIN-1512 | [D38](#d38) | Добавить proc-macro expansion size checks | API | candidate | M | G-API |
| 1777 | LIN-1513 | [D38](#d38) | Измерить typed row decode overhead | PERF | candidate | M | G-PERF |
| 1778 | LIN-1514 | [D38](#d38) | Измерить derive-generated allocation count | PERF | candidate | M | G-PERF |
| 1779 | LIN-1515 | [D38](#d38) | Измерить typed batch encode throughput | PERF | candidate | M | G-PERF |
| 1780 | LIN-1516 | [D38](#d38) | Измерить schema descriptor reuse | PERF | candidate | M | G-PERF |
| 1781 | LIN-1517 | [D38](#d38) | Проверить typed/raw roundtrip values | CORRECTNESS | qualification | S | G-CORRECT |
| 1782 | LIN-1518 | [D38](#d38) | Проверить typed/null schema compatibility | CORRECTNESS | qualification | S | G-CORRECT |
| 1783 | LIN-1519 | [D38](#d38) | Проверить typed query schema drift | CORRECTNESS | qualification | S | G-CORRECT |
| 1784 | LIN-1520 | [D38](#d38) | Проверить typed error field provenance | CORRECTNESS | qualification | S | G-CORRECT |
| 1785 | LIN-1545 | [D39](#d39) | Улучшить backup CLI progress | API | candidate | M | G-API |
| 1786 | LIN-1546 | [D39](#d39) | Улучшить follower CLI status | API | candidate | M | G-API |
| 1787 | LIN-1547 | [D39](#d39) | Улучшить checkpoint CLI status | API | candidate | M | G-API |
| 1788 | LIN-1548 | [D39](#d39) | Улучшить recover CLI guidance | API | candidate | M | G-API |
| 1789 | LIN-1549 | [D39](#d39) | Добавить stable statistics schema | API | candidate | M | G-API |
| 1790 | LIN-1550 | [D39](#d39) | Добавить version capability output | API | candidate | M | G-API |
| 1791 | LIN-1551 | [D39](#d39) | Добавить feature availability output | API | candidate | M | G-API |
| 1792 | LIN-1552 | [D39](#d39) | Добавить endpoint redaction in errors | API | candidate | M | G-API |
| 1793 | LIN-1553 | [D39](#d39) | Проверить shell quoting examples | CORRECTNESS | qualification | S | G-CORRECT |
| 1794 | LIN-1554 | [D39](#d39) | Проверить Unicode terminal output | CORRECTNESS | qualification | S | G-CORRECT |
| 1795 | LIN-1555 | [D39](#d39) | Проверить Windows path arguments | CORRECTNESS | qualification | S | G-CORRECT |
| 1796 | LIN-1556 | [D39](#d39) | Проверить symlink data directory policy | CORRECTNESS | qualification | S | G-CORRECT |
| 1797 | LIN-1557 | [D39](#d39) | Добавить CLI smoke fixture packs | API | candidate | M | G-API |
| 1798 | LIN-1558 | [D39](#d39) | Добавить CLI JSON schema validation | API | candidate | M | G-API |
| 1799 | LIN-1559 | [D39](#d39) | Добавить CLI backwards compatibility checks | API | candidate | M | G-API |
| 1800 | LIN-1560 | [D39](#d39) | Добавить completion generation command | API | candidate | M | G-API |
| 1801 | LIN-1585 | [D40](#d40) | Спроектировать structured warning channel | FEATURE | candidate | L | G-FEATURE |
| 1802 | LIN-1586 | [D40](#d40) | Спроектировать warning severity policy | FEATURE | candidate | L | G-FEATURE |
| 1803 | LIN-1587 | [D40](#d40) | Спроектировать warning suppression scopes | FEATURE | candidate | L | G-FEATURE |
| 1804 | LIN-1588 | [D40](#d40) | Спроектировать warning aggregation budget | FEATURE | candidate | L | G-FEATURE |
| 1805 | LIN-1589 | [D40](#d40) | Добавить error-code compatibility corpus | VERIFY | qualification | S | G-VERIFY |
| 1806 | LIN-1590 | [D40](#d40) | Добавить parser diagnostic golden corpus | VERIFY | qualification | S | G-VERIFY |
| 1807 | LIN-1591 | [D40](#d40) | Добавить storage diagnostic golden corpus | VERIFY | qualification | S | G-VERIFY |
| 1808 | LIN-1592 | [D40](#d40) | Добавить replication diagnostic golden corpus | VERIFY | qualification | S | G-VERIFY |
| 1809 | LIN-1593 | [D40](#d40) | Измерить cold error path allocation | PERF | candidate | M | G-PERF |
| 1810 | LIN-1594 | [D40](#d40) | Измерить success-path diagnostic overhead | PERF | candidate | M | G-PERF |
| 1811 | LIN-1595 | [D40](#d40) | Измерить verbose trace opt-in cost | PERF | candidate | M | G-PERF |
| 1812 | LIN-1596 | [D40](#d40) | Измерить batch error aggregation cost | PERF | candidate | M | G-PERF |
| 1813 | LIN-1597 | [D40](#d40) | Проверить CLI/library error parity | CORRECTNESS | qualification | S | G-CORRECT |
| 1814 | LIN-1598 | [D40](#d40) | Проверить async/library error parity | CORRECTNESS | qualification | S | G-CORRECT |
| 1815 | LIN-1599 | [D40](#d40) | Проверить GPU/CPU error classification | CORRECTNESS | qualification | S | G-CORRECT |
| 1816 | LIN-1600 | [D40](#d40) | Проверить retryability error classification | CORRECTNESS | qualification | S | G-CORRECT |
| 1817 | LIN-1625 | [D41](#d41) | Добавить replica reconnect metrics | OPS | candidate | M | G-OPS |
| 1818 | LIN-1626 | [D41](#d41) | Добавить stale snapshot metrics | OPS | candidate | M | G-OPS |
| 1819 | LIN-1627 | [D41](#d41) | Добавить embedding failure metrics | OPS | candidate | M | G-OPS |
| 1820 | LIN-1628 | [D41](#d41) | Добавить compaction pause metrics | OPS | candidate | M | G-OPS |
| 1821 | LIN-1629 | [D41](#d41) | Спроектировать health status snapshot | OPS | candidate | L | G-OPS |
| 1822 | LIN-1630 | [D41](#d41) | Спроектировать readiness status semantics | OPS | candidate | L | G-OPS |
| 1823 | LIN-1631 | [D41](#d41) | Спроектировать per-collection statistics export | OPS | candidate | L | G-OPS |
| 1824 | LIN-1632 | [D41](#d41) | Спроектировать resettable diagnostic counters | OPS | candidate | L | G-OPS |
| 1825 | LIN-1633 | [D41](#d41) | Проверить telemetry during panic unwind | CORRECTNESS | qualification | S | G-CORRECT |
| 1826 | LIN-1634 | [D41](#d41) | Проверить concurrent counter consistency | CORRECTNESS | qualification | S | G-CORRECT |
| 1827 | LIN-1635 | [D41](#d41) | Проверить metrics sink failure isolation | CORRECTNESS | qualification | S | G-CORRECT |
| 1828 | LIN-1636 | [D41](#d41) | Проверить instrumentation after shutdown | CORRECTNESS | qualification | S | G-CORRECT |
| 1829 | LIN-1637 | [D41](#d41) | Добавить observability schema version | OPS | candidate | M | G-OPS |
| 1830 | LIN-1638 | [D41](#d41) | Добавить telemetry integration fixtures | VERIFY | qualification | S | G-VERIFY |
| 1831 | LIN-1639 | [D41](#d41) | Добавить profiler command cookbook | OPS | candidate | M | G-OPS |
| 1832 | LIN-1640 | [D41](#d41) | Добавить trace-to-benchmark provenance links | OPS | candidate | M | G-OPS |
| 1833 | LIN-1665 | [D42](#d42) | Спроектировать seeded replayable fuzz runs | VERIFY | qualification | L | G-VERIFY |
| 1834 | LIN-1666 | [D42](#d42) | Спроектировать persisted failure corpus | VERIFY | qualification | L | G-VERIFY |
| 1835 | LIN-1667 | [D42](#d42) | Спроектировать cross-platform corpus exchange | VERIFY | qualification | L | G-VERIFY |
| 1836 | LIN-1668 | [D42](#d42) | Спроектировать minimized-case metadata | VERIFY | qualification | L | G-VERIFY |
| 1837 | LIN-1669 | [D42](#d42) | Добавить libFuzzer parser target | VERIFY | qualification | S | G-VERIFY |
| 1838 | LIN-1670 | [D42](#d42) | Добавить libFuzzer WAL target | VERIFY | qualification | S | G-VERIFY |
| 1839 | LIN-1671 | [D42](#d42) | Добавить libFuzzer snapshot target | VERIFY | qualification | S | G-VERIFY |
| 1840 | LIN-1672 | [D42](#d42) | Добавить libFuzzer replication target | VERIFY | qualification | S | G-VERIFY |
| 1841 | LIN-1673 | [D42](#d42) | Ограничить fuzz resource usage | VERIFY | qualification | S | G-VERIFY |
| 1842 | LIN-1674 | [D42](#d42) | Исключить flaky time assertions | VERIFY | qualification | S | G-VERIFY |
| 1843 | LIN-1675 | [D42](#d42) | Выявлять order-dependent tests | VERIFY | qualification | S | G-VERIFY |
| 1844 | LIN-1676 | [D42](#d42) | Проверять cleanup после injected errors | VERIFY | qualification | S | G-VERIFY |
| 1845 | LIN-1677 | [D42](#d42) | Добавить coverage-gap reporting | VERIFY | qualification | S | G-VERIFY |
| 1846 | LIN-1678 | [D42](#d42) | Добавить corpus retention policy | VERIFY | qualification | S | G-VERIFY |
| 1847 | LIN-1679 | [D42](#d42) | Добавить deterministic test clock | VERIFY | qualification | S | G-VERIFY |
| 1848 | LIN-1680 | [D42](#d42) | Добавить deterministic test RNG injection | VERIFY | qualification | S | G-VERIFY |
| 1849 | LIN-1705 | [D43](#d43) | Спроектировать pluggable filesystem shim | VERIFY | qualification | L | G-VERIFY |
| 1850 | LIN-1706 | [D43](#d43) | Спроектировать deterministic I/O fault schedule | VERIFY | qualification | L | G-VERIFY |
| 1851 | LIN-1707 | [D43](#d43) | Спроектировать crash subprocess harness | VERIFY | qualification | L | G-VERIFY |
| 1852 | LIN-1708 | [D43](#d43) | Спроектировать durable oracle journal | VERIFY | qualification | L | G-VERIFY |
| 1853 | LIN-1709 | [D43](#d43) | Проверить short write всех frame segments | VERIFY | qualification | S | G-VERIFY |
| 1854 | LIN-1710 | [D43](#d43) | Проверить sync failure на group commit | VERIFY | qualification | S | G-VERIFY |
| 1855 | LIN-1711 | [D43](#d43) | Проверить interrupted temp-file cleanup | VERIFY | qualification | S | G-VERIFY |
| 1856 | LIN-1712 | [D43](#d43) | Проверить failed directory sync recovery | VERIFY | qualification | S | G-VERIFY |
| 1857 | LIN-1713 | [D43](#d43) | Добавить power-loss simulation scenarios | VERIFY | qualification | S | G-VERIFY |
| 1858 | LIN-1714 | [D43](#d43) | Добавить checkpoint chaos matrix | VERIFY | qualification | S | G-VERIFY |
| 1859 | LIN-1715 | [D43](#d43) | Добавить replication chaos matrix | VERIFY | qualification | S | G-VERIFY |
| 1860 | LIN-1716 | [D43](#d43) | Добавить cross-version crash fixtures | VERIFY | qualification | S | G-VERIFY |
| 1861 | LIN-1717 | [D43](#d43) | Сохранять fault schedule artifacts | VERIFY | qualification | S | G-VERIFY |
| 1862 | LIN-1718 | [D43](#d43) | Сохранять failed frame hexdumps | VERIFY | qualification | S | G-VERIFY |
| 1863 | LIN-1719 | [D43](#d43) | Сохранять recovery decision logs | VERIFY | qualification | S | G-VERIFY |
| 1864 | LIN-1720 | [D43](#d43) | Сохранять minimized crash reproducers | VERIFY | qualification | S | G-VERIFY |
| 1865 | LIN-1745 | [D44](#d44) | Проверять fixture cleanup in CI | OPS | candidate | M | G-OPS |
| 1866 | LIN-1746 | [D44](#d44) | Проверять orphan child processes | OPS | candidate | M | G-OPS |
| 1867 | LIN-1747 | [D44](#d44) | Проверять hanging test time budgets | OPS | candidate | M | G-OPS |
| 1868 | LIN-1748 | [D44](#d44) | Проверять artifact schema compatibility | OPS | candidate | M | G-OPS |
| 1869 | LIN-1749 | [D44](#d44) | Добавить Miri safe-core job | OPS | candidate | M | G-OPS |
| 1870 | LIN-1750 | [D44](#d44) | Добавить sanitizer memory job | OPS | candidate | M | G-OPS |
| 1871 | LIN-1751 | [D44](#d44) | Добавить fuzz smoke job | VERIFY | qualification | S | G-VERIFY |
| 1872 | LIN-1752 | [D44](#d44) | Добавить cargo audit policy review | OPS | candidate | M | G-OPS |
| 1873 | LIN-1753 | [D44](#d44) | Ограничить CI concurrency interference | CORRECTNESS | qualification | M | G-CORRECT |
| 1874 | LIN-1754 | [D44](#d44) | Кэшировать bundled peer build correctly | OPS | candidate | M | G-OPS |
| 1875 | LIN-1755 | [D44](#d44) | Проверять reproducible release build | OPS | candidate | M | G-OPS |
| 1876 | LIN-1756 | [D44](#d44) | Проверять clean-checkout test execution | OPS | candidate | M | G-OPS |
| 1877 | LIN-1757 | [D44](#d44) | Добавить regression bisect command | OPS | candidate | M | G-OPS |
| 1878 | LIN-1758 | [D44](#d44) | Добавить stale baseline detection | OPS | candidate | M | G-OPS |
| 1879 | LIN-1759 | [D44](#d44) | Добавить platform support badges evidence | OPS | candidate | M | G-OPS |
| 1880 | LIN-1760 | [D44](#d44) | Добавить skipped-test reason reporting | OPS | candidate | M | G-OPS |
| 1881 | LIN-1785 | [D45](#d45) | Добавить resource-limit receipts | FEATURE | candidate | M | G-FEATURE |
| 1882 | LIN-1786 | [D45](#d45) | Добавить deny-by-default unsafe operations | FEATURE | candidate | M | G-FEATURE |
| 1883 | LIN-1787 | [D45](#d45) | Добавить explicit network model permissions | FEATURE | candidate | M | G-FEATURE |
| 1884 | LIN-1788 | [D45](#d45) | Добавить import file-root restrictions | FEATURE | candidate | M | G-FEATURE |
| 1885 | LIN-1789 | [D45](#d45) | Проверить C ABI pointer validation design | CORRECTNESS | qualification | S | G-CORRECT |
| 1886 | LIN-1790 | [D45](#d45) | Проверить unsafe GPU buffer boundaries | CORRECTNESS | qualification | S | G-CORRECT |
| 1887 | LIN-1791 | [D45](#d45) | Проверить mmap bounds arithmetic | CORRECTNESS | qualification | S | G-CORRECT |
| 1888 | LIN-1792 | [D45](#d45) | Проверить SIMD unaligned tail safety | CORRECTNESS | qualification | S | G-CORRECT |
| 1889 | LIN-1793 | [D45](#d45) | Добавить security regression corpus | VERIFY | qualification | S | G-VERIFY |
| 1890 | LIN-1794 | [D45](#d45) | Добавить responsible disclosure guidance | FEATURE | candidate | M | G-FEATURE |
| 1891 | LIN-1795 | [D45](#d45) | Добавить dependency provenance inventory | FEATURE | candidate | M | G-FEATURE |
| 1892 | LIN-1796 | [D45](#d45) | Добавить offline execution qualification | FEATURE | candidate | M | G-FEATURE |
| 1893 | LIN-1797 | [D45](#d45) | Проверить no implicit executable loading | CORRECTNESS | qualification | S | G-CORRECT |
| 1894 | LIN-1798 | [D45](#d45) | Проверить no silent external requests | CORRECTNESS | qualification | S | G-CORRECT |
| 1895 | LIN-1799 | [D45](#d45) | Проверить CLI history secret filtering | CORRECTNESS | qualification | S | G-CORRECT |
| 1896 | LIN-1800 | [D45](#d45) | Проверить logs bounded-value formatting | CORRECTNESS | qualification | S | G-CORRECT |
| 1897 | LIN-1825 | [D46](#d46) | Проверить unknown catalog versions | CORRECTNESS | qualification | S | G-CORRECT |
| 1898 | LIN-1826 | [D46](#d46) | Проверить duplicate collection declarations | CORRECTNESS | qualification | S | G-CORRECT |
| 1899 | LIN-1827 | [D46](#d46) | Проверить reserved field policy | CORRECTNESS | qualification | S | G-CORRECT |
| 1900 | LIN-1828 | [D46](#d46) | Проверить cyclic schema references | CORRECTNESS | qualification | S | G-CORRECT |
| 1901 | LIN-1829 | [D46](#d46) | Измерить catalog clone footprint | PERF | candidate | M | G-PERF |
| 1902 | LIN-1830 | [D46](#d46) | Измерить interned catalog identifiers | PERF | candidate | M | G-PERF |
| 1903 | LIN-1831 | [D46](#d46) | Измерить schema lookup cache | PERF | candidate | M | G-PERF |
| 1904 | LIN-1832 | [D46](#d46) | Измерить prepared schema invalidation | PERF | candidate | M | G-PERF |
| 1905 | LIN-1833 | [D46](#d46) | Проверить catalog replay consistency | CORRECTNESS | qualification | S | G-CORRECT |
| 1906 | LIN-1834 | [D46](#d46) | Проверить catalog snapshot roundtrip | CORRECTNESS | qualification | S | G-CORRECT |
| 1907 | LIN-1835 | [D46](#d46) | Проверить catalog follower propagation | CORRECTNESS | qualification | S | G-CORRECT |
| 1908 | LIN-1836 | [D46](#d46) | Проверить catalog rollback consistency | CORRECTNESS | qualification | S | G-CORRECT |
| 1909 | LIN-1837 | [D46](#d46) | Добавить migration fixture archive | FEATURE | candidate | M | G-FEATURE |
| 1910 | LIN-1838 | [D46](#d46) | Добавить downgrade feasibility report | FEATURE | candidate | M | G-FEATURE |
| 1911 | LIN-1839 | [D46](#d46) | Добавить irreversible migration labels | FEATURE | candidate | M | G-FEATURE |
| 1912 | LIN-1840 | [D46](#d46) | Добавить schema evolution compatibility matrix | FEATURE | candidate | L | G-FEATURE |
| 1913 | LIN-1865 | [D47](#d47) | Спроектировать resumable import offsets | FEATURE | candidate | L | G-FEATURE |
| 1914 | LIN-1866 | [D47](#d47) | Спроектировать atomic import batches | FEATURE | candidate | L | G-FEATURE |
| 1915 | LIN-1867 | [D47](#d47) | Спроектировать idempotent import identities | FEATURE | candidate | L | G-FEATURE |
| 1916 | LIN-1868 | [D47](#d47) | Спроектировать deterministic export ordering | FEATURE | candidate | L | G-FEATURE |
| 1917 | LIN-1869 | [D47](#d47) | Ограничить import row bytes | CORRECTNESS | qualification | M | G-CORRECT |
| 1918 | LIN-1870 | [D47](#d47) | Ограничить import nesting depth | CORRECTNESS | qualification | M | G-CORRECT |
| 1919 | LIN-1871 | [D47](#d47) | Ограничить import decompressed bytes | CORRECTNESS | qualification | M | G-CORRECT |
| 1920 | LIN-1872 | [D47](#d47) | Ограничить import queued batches | CORRECTNESS | qualification | M | G-CORRECT |
| 1921 | LIN-1873 | [D47](#d47) | Измерить columnar import throughput | PERF | candidate | M | G-PERF |
| 1922 | LIN-1874 | [D47](#d47) | Измерить CSV parser allocation | PERF | candidate | M | G-PERF |
| 1923 | LIN-1875 | [D47](#d47) | Измерить large export memory bound | PERF | candidate | M | G-PERF |
| 1924 | LIN-1876 | [D47](#d47) | Измерить import index maintenance cost | PERF | candidate | M | G-PERF |
| 1925 | LIN-1877 | [D47](#d47) | Добавить peer dataset exchange fixtures | VERIFY | qualification | S | G-VERIFY |
| 1926 | LIN-1878 | [D47](#d47) | Добавить format roundtrip fixtures | VERIFY | qualification | S | G-VERIFY |
| 1927 | LIN-1879 | [D47](#d47) | Добавить cross-version logical export checks | FEATURE | candidate | M | G-FEATURE |
| 1928 | LIN-1880 | [D47](#d47) | Добавить import provenance manifest | FEATURE | candidate | M | G-FEATURE |
| 1929 | LIN-1905 | [D48](#d48) | Измерить open/close churn | PERF | candidate | M | G-PERF |
| 1930 | LIN-1906 | [D48](#d48) | Измерить long-running RSS growth | PERF | candidate | M | G-PERF |
| 1931 | LIN-1907 | [D48](#d48) | Измерить repeated checkpoint fragmentation | PERF | candidate | M | G-PERF |
| 1932 | LIN-1908 | [D48](#d48) | Измерить repeated replica reconnect leakage | PERF | candidate | M | G-PERF |
| 1933 | LIN-1909 | [D48](#d48) | Добавить config effective-value report | OPS | candidate | M | G-OPS |
| 1934 | LIN-1910 | [D48](#d48) | Добавить config validation command | OPS | candidate | M | G-OPS |
| 1935 | LIN-1911 | [D48](#d48) | Добавить config schema version | OPS | candidate | M | G-OPS |
| 1936 | LIN-1912 | [D48](#d48) | Добавить environment override provenance | OPS | candidate | M | G-OPS |
| 1937 | LIN-1913 | [D48](#d48) | Создать disk-full runbook | OPS | candidate | M | G-OPS |
| 1938 | LIN-1914 | [D48](#d48) | Создать corruption investigation runbook | OPS | candidate | M | G-OPS |
| 1939 | LIN-1915 | [D48](#d48) | Создать follower lag runbook | OPS | candidate | M | G-OPS |
| 1940 | LIN-1916 | [D48](#d48) | Создать safe upgrade runbook | OPS | candidate | M | G-OPS |
| 1941 | LIN-1917 | [D48](#d48) | Проверить system suspend/resume behavior | CORRECTNESS | qualification | S | G-CORRECT |
| 1942 | LIN-1918 | [D48](#d48) | Проверить clock discontinuity handling | CORRECTNESS | qualification | S | G-CORRECT |
| 1943 | LIN-1919 | [D48](#d48) | Проверить timezone-independent storage | CORRECTNESS | qualification | S | G-CORRECT |
| 1944 | LIN-1920 | [D48](#d48) | Проверить crash-after-shutdown-request recovery | CORRECTNESS | qualification | S | G-CORRECT |
| 1945 | LIN-1933 | [D49](#d49) | Проверить cross-compilation target matrix | CORRECTNESS | qualification | S | G-CORRECT |
| 1946 | LIN-1934 | [D49](#d49) | Проверить libc/platform conditional imports | CORRECTNESS | qualification | S | G-CORRECT |
| 1947 | LIN-1935 | [D49](#d49) | Проверить optional compression isolation | CORRECTNESS | qualification | S | G-CORRECT |
| 1948 | LIN-1936 | [D49](#d49) | Проверить optional neural backend isolation | CORRECTNESS | qualification | S | G-CORRECT |
| 1949 | LIN-1937 | [D49](#d49) | Измерить binary size sections | PERF | candidate | M | G-PERF |
| 1950 | LIN-1938 | [D49](#d49) | Измерить LTO variants | PERF | candidate | M | G-PERF |
| 1951 | LIN-1939 | [D49](#d49) | Измерить codegen-units variants | PERF | candidate | M | G-PERF |
| 1952 | LIN-1940 | [D49](#d49) | Измерить PGO workload sensitivity | PERF | candidate | M | G-PERF |
| 1953 | LIN-1941 | [D49](#d49) | Проверить reproducible artifact hashes | CORRECTNESS | qualification | S | G-CORRECT |
| 1954 | LIN-1942 | [D49](#d49) | Проверить release debug-symbol separation | CORRECTNESS | qualification | S | G-CORRECT |
| 1955 | LIN-1943 | [D49](#d49) | Проверить source archive completeness | CORRECTNESS | qualification | S | G-CORRECT |
| 1956 | LIN-1944 | [D49](#d49) | Проверить cargo package contents | CORRECTNESS | qualification | S | G-CORRECT |
| 1957 | LIN-1945 | [D49](#d49) | Добавить signed release manifest | OPS | candidate | M | G-OPS |
| 1958 | LIN-1946 | [D49](#d49) | Добавить SBOM generation | OPS | candidate | M | G-OPS |
| 1959 | LIN-1947 | [D49](#d49) | Добавить build provenance receipts | OPS | candidate | M | G-OPS |
| 1960 | LIN-1948 | [D49](#d49) | Добавить checksum verification instructions | OPS | candidate | M | G-OPS |
| 1961 | LIN-1949 | [D49](#d49) | Спроектировать native Python wheel matrix | OPS | candidate | L | G-OPS |
| 1962 | LIN-1950 | [D49](#d49) | Спроектировать C library packaging | OPS | candidate | L | G-OPS |
| 1963 | LIN-1951 | [D49](#d49) | Спроектировать WASM package split | OPS | candidate | L | G-OPS |
| 1964 | LIN-1952 | [D49](#d49) | Спроектировать static linking support policy | OPS | candidate | L | G-OPS |
| 1965 | LIN-1953 | [D49](#d49) | Проверить downstream global allocator compatibility | CORRECTNESS | qualification | S | G-CORRECT |
| 1966 | LIN-1954 | [D49](#d49) | Проверить target CPU feature portability | CORRECTNESS | qualification | S | G-CORRECT |
| 1967 | LIN-1955 | [D49](#d49) | Проверить runtime SIMD dispatch gates | CORRECTNESS | qualification | S | G-CORRECT |
| 1968 | LIN-1956 | [D49](#d49) | Проверить compile-time cfg consistency | CORRECTNESS | qualification | S | G-CORRECT |
| 1969 | LIN-1957 | [D49](#d49) | Добавить clean-room installation smoke | OPS | candidate | M | G-OPS |
| 1970 | LIN-1958 | [D49](#d49) | Добавить offline dependency build guide | OPS | candidate | M | G-OPS |
| 1971 | LIN-1959 | [D49](#d49) | Добавить toolchain rollback procedure | OPS | candidate | M | G-OPS |
| 1972 | LIN-1960 | [D49](#d49) | Добавить release regression checklist | OPS | candidate | M | G-OPS |
| 1973 | LIN-1973 | [D50](#d50) | Создать benchmark interpretation guide | DOC | candidate | S | G-DOC |
| 1974 | LIN-1974 | [D50](#d50) | Создать matched-peer fixture guide | DOC | candidate | S | G-DOC |
| 1975 | LIN-1975 | [D50](#d50) | Создать raw artifact navigation | DOC | candidate | S | G-DOC |
| 1976 | LIN-1976 | [D50](#d50) | Создать current performance gap dashboard | DOC | candidate | S | G-DOC |
| 1977 | LIN-1977 | [D50](#d50) | Создать query migration cookbook | DOC | candidate | S | G-DOC |
| 1978 | LIN-1978 | [D50](#d50) | Создать schema migration cookbook | DOC | candidate | S | G-DOC |
| 1979 | LIN-1979 | [D50](#d50) | Создать embedding migration cookbook | DOC | candidate | S | G-DOC |
| 1980 | LIN-1980 | [D50](#d50) | Создать backup migration cookbook | DOC | candidate | S | G-DOC |
| 1981 | LIN-1981 | [D50](#d50) | Описать memory ownership API | DOC | candidate | S | G-DOC |
| 1982 | LIN-1982 | [D50](#d50) | Описать cancellation behavior API | DOC | candidate | S | G-DOC |
| 1983 | LIN-1983 | [D50](#d50) | Описать async thread usage | DOC | candidate | S | G-DOC |
| 1984 | LIN-1984 | [D50](#d50) | Описать cursor snapshot semantics | DOC | candidate | S | G-DOC |
| 1985 | LIN-1985 | [D50](#d50) | Добавить runnable ingestion examples | DOC | candidate | S | G-DOC |
| 1986 | LIN-1986 | [D50](#d50) | Добавить runnable analytics examples | DOC | candidate | S | G-DOC |
| 1987 | LIN-1987 | [D50](#d50) | Добавить runnable graph examples | DOC | candidate | S | G-DOC |
| 1988 | LIN-1988 | [D50](#d50) | Добавить runnable hybrid-search examples | DOC | candidate | S | G-DOC |
| 1989 | LIN-1989 | [D50](#d50) | Описать optional backend requirements | DOC | candidate | S | G-DOC |
| 1990 | LIN-1990 | [D50](#d50) | Описать dedicated benchmark server setup | DOC | candidate | S | G-DOC |
| 1991 | LIN-1991 | [D50](#d50) | Описать air-gapped deployment | DOC | candidate | S | G-DOC |
| 1992 | LIN-1992 | [D50](#d50) | Описать resource sizing methodology | DOC | candidate | S | G-DOC |
| 1993 | LIN-1993 | [D50](#d50) | Связать roadmap items с evidence | DOC | candidate | S | G-DOC |
| 1994 | LIN-1994 | [D50](#d50) | Связать fixed defects с regressions | DOC | candidate | S | G-DOC |
| 1995 | LIN-1995 | [D50](#d50) | Связать releases с benchmark sources | DOC | candidate | S | G-DOC |
| 1996 | LIN-1996 | [D50](#d50) | Связать claims с validation scope | DOC | candidate | S | G-DOC |
| 1997 | LIN-1997 | [D50](#d50) | Ввести roadmap triage policy | DOC | candidate | S | G-DOC |
| 1998 | LIN-1998 | [D50](#d50) | Ввести rejected-experiment registry | DOC | candidate | S | G-DOC |
| 1999 | LIN-1999 | [D50](#d50) | Ввести architecture decision records | DOC | candidate | S | G-DOC |
| 2000 | LIN-2000 | [D50](#d50) | Ввести completion evidence checklist | DOC | candidate | S | G-DOC |

## Области и привязка к коду

Ниже 50 областей по 40 пунктов. Зависимости областей — контекст для чтения и qualification, а не требование полностью закончить все 40 задач одной области перед другой. Взаимные связи архитектурных областей не являются циклом исполнения; реальные зависимости отдельных задач перечислены отдельно.

<a id="d01"></a>

### D01. Бенчмарки и доказательства

Модули: [benches/compare.rs](../benches/compare.rs), [scripts/bench-peers.py](../scripts/bench-peers.py), [docs/benchmark-contract.md](../docs/benchmark-contract.md). 40 пунктов: `LIN-0001`–`LIN-0040`.

<a id="d02"></a>

### D02. Формат WAL и checksum

Модули: [src/persist.rs](../src/persist.rs), [src/crc.rs](../src/crc.rs), [tests/persist.rs](../tests/persist.rs). 40 пунктов: `LIN-0041`–`LIN-0080`.

Контекст: [D01](#d01).

<a id="d03"></a>

### D03. Durable I/O и sync

Модули: [src/persist.rs](../src/persist.rs), [src/ship.rs](../src/ship.rs), [tests/persist.rs](../tests/persist.rs). 40 пунктов: `LIN-0081`–`LIN-0120`.

Контекст: [D01](#d01), [D02](#d02).

<a id="d04"></a>

### D04. Replay и восстановление

Модули: [src/persist.rs](../src/persist.rs), [src/store.rs](../src/store.rs), [tests/persist.rs](../tests/persist.rs). 40 пунктов: `LIN-0121`–`LIN-0160`.

Контекст: [D02](#d02), [D03](#d03).

<a id="d05"></a>

### D05. Snapshot и checkpoint

Модули: [src/persist.rs](../src/persist.rs), [src/store.rs](../src/store.rs), [src/cold.rs](../src/cold.rs). 40 пунктов: `LIN-0161`–`LIN-0200`.

Контекст: [D02](#d02), [D03](#d03), [D04](#d04).

<a id="d06"></a>

### D06. Транзакции и atomicity

Модули: [src/exec.rs](../src/exec.rs), [src/store.rs](../src/store.rs), [tests/exec.rs](../tests/exec.rs). 40 пунктов: `LIN-0201`–`LIN-0240`.

Контекст: [D02](#d02), [D04](#d04).

<a id="d07"></a>

### D07. Mutations и bulk ingestion

Модули: [src/exec.rs](../src/exec.rs), [src/store.rs](../src/store.rs), [tests/exec.rs](../tests/exec.rs). 40 пунктов: `LIN-0241`–`LIN-0280`.

Контекст: [D06](#d06), [D08](#d08), [D10](#d10).

<a id="d08"></a>

### D08. Cell и строковое представление

Модули: [src/store.rs](../src/store.rs), [src/row.rs](../src/row.rs), [src/ast.rs](../src/ast.rs). 40 пунктов: `LIN-0281`–`LIN-0320`.

Контекст: [D01](#d01).

<a id="d09"></a>

### D09. Владение и пулы памяти

Модули: [src/row.rs](../src/row.rs), [src/batch.rs](../src/batch.rs), [src/cursor.rs](../src/cursor.rs). 40 пунктов: `LIN-0321`–`LIN-0360`.

Контекст: [D08](#d08).

<a id="d10"></a>

### D10. Scalar indexes

Модули: [src/index.rs](../src/index.rs), [src/store.rs](../src/store.rs), [tests/query.rs](../tests/query.rs). 40 пунктов: `LIN-0361`–`LIN-0400`.

Контекст: [D08](#d08), [D11](#d11).

<a id="d11"></a>

### D11. Ограничения и числовые ключи

Модули: [src/index.rs](../src/index.rs), [src/check.rs](../src/check.rs), [tests/exec.rs](../tests/exec.rs). 40 пунктов: `LIN-0401`–`LIN-0440`.

Контекст: [D08](#d08), [D13](#d13).

<a id="d12"></a>

### D12. Парсер и синтаксис

Модули: [src/parse.rs](../src/parse.rs), [src/ast.rs](../src/ast.rs), [tests/lang.rs](../tests/lang.rs). 40 пунктов: `LIN-0441`–`LIN-0480`.

Контекст: [D13](#d13).

<a id="d13"></a>

### D13. Type checking и semantics

Модули: [src/check.rs](../src/check.rs), [src/catalog.rs](../src/catalog.rs), [tests/lang.rs](../tests/lang.rs). 40 пунктов: `LIN-0481`–`LIN-0520`.

Контекст: [D08](#d08), [D46](#d46).

<a id="d14"></a>

### D14. Логический planner

Модули: [src/plan.rs](../src/plan.rs), [src/explain.rs](../src/explain.rs), [tests/query.rs](../tests/query.rs). 40 пунктов: `LIN-0521`–`LIN-0560`.

Контекст: [D13](#d13).

<a id="d15"></a>

### D15. Статистика и cost model

Модули: [src/plan.rs](../src/plan.rs), [src/catalog.rs](../src/catalog.rs), [src/index.rs](../src/index.rs). 40 пунктов: `LIN-0561`–`LIN-0600`.

Контекст: [D14](#d14), [D10](#d10).

<a id="d16"></a>

### D16. Execution pipeline

Модули: [src/exec.rs](../src/exec.rs), [src/row.rs](../src/row.rs), [src/batch.rs](../src/batch.rs). 40 пунктов: `LIN-0601`–`LIN-0640`.

Контекст: [D13](#d13), [D14](#d14).

<a id="d17"></a>

### D17. Cursors и streaming

Модули: [src/cursor.rs](../src/cursor.rs), [src/query.rs](../src/query.rs), [src/async_db.rs](../src/async_db.rs). 40 пунктов: `LIN-0641`–`LIN-0680`.

Контекст: [D16](#d16), [D34](#d34).

<a id="d18"></a>

### D18. Колонки и RecordBatch

Модули: [src/batch.rs](../src/batch.rs), [src/row.rs](../src/row.rs), [src/query.rs](../src/query.rs). 40 пунктов: `LIN-0681`–`LIN-0720`.

Контекст: [D08](#d08).

<a id="d19"></a>

### D19. Aggregations и GROUP BY

Модули: [src/exec.rs](../src/exec.rs), [src/store.rs](../src/store.rs), [tests/query.rs](../tests/query.rs). 40 пунктов: `LIN-0721`–`LIN-0760`.

Контекст: [D08](#d08), [D13](#d13).

<a id="d20"></a>

### D20. Sorting и Top-K

Модули: [src/exec.rs](../src/exec.rs), [src/cursor.rs](../src/cursor.rs), [src/store.rs](../src/store.rs). 40 пунктов: `LIN-0761`–`LIN-0800`.

Контекст: [D08](#d08), [D13](#d13).

<a id="d21"></a>

### D21. Joins и relational composition

Модули: [src/exec.rs](../src/exec.rs), [src/cursor.rs](../src/cursor.rs), [src/batch.rs](../src/batch.rs). 40 пунктов: `LIN-0801`–`LIN-0840`.

Контекст: [D08](#d08), [D14](#d14).

<a id="d22"></a>

### D22. Window functions

Модули: [src/window.rs](../src/window.rs), [src/exec.rs](../src/exec.rs), [tests/query.rs](../tests/query.rs). 40 пунктов: `LIN-0841`–`LIN-0880`.

Контекст: [D08](#d08), [D19](#d19), [D20](#d20).

<a id="d23"></a>

### D23. FTS build и tokenizer

Модули: [src/fts.rs](../src/fts.rs), [src/catalog.rs](../src/catalog.rs), [tests/query.rs](../tests/query.rs). 40 пунктов: `LIN-0881`–`LIN-0920`.

Контекст: [D08](#d08), [D46](#d46).

<a id="d24"></a>

### D24. FTS query и scoring

Модули: [src/fts.rs](../src/fts.rs), [src/exec.rs](../src/exec.rs), [src/cursor.rs](../src/cursor.rs). 40 пунктов: `LIN-0921`–`LIN-0960`.

Контекст: [D23](#d23).

<a id="d25"></a>

### D25. FTS mutation и compaction

Модули: [src/fts.rs](../src/fts.rs), [src/store.rs](../src/store.rs), [tests/delete_readback.rs](../tests/delete_readback.rs). 40 пунктов: `LIN-0961`–`LIN-1000`.

Контекст: [D06](#d06), [D23](#d23).

<a id="d26"></a>

### D26. Hashing embeddings

Модули: [src/embed.rs](../src/embed.rs), [src/exec.rs](../src/exec.rs), [tests/embed.rs](../tests/embed.rs). 40 пунктов: `LIN-1001`–`LIN-1040`.

Контекст: [D08](#d08), [D13](#d13).

<a id="d27"></a>

### D27. ANN и HNSW

Модули: [src/embed.rs](../src/embed.rs), [src/store.rs](../src/store.rs), [src/exec.rs](../src/exec.rs). 40 пунктов: `LIN-1041`–`LIN-1080`.

Контекст: [D26](#d26), [D28](#d28).

<a id="d28"></a>

### D28. Vector search semantics

Модули: [src/embed.rs](../src/embed.rs), [src/exec.rs](../src/exec.rs), [src/cursor.rs](../src/cursor.rs). 40 пунктов: `LIN-1081`–`LIN-1120`.

Контекст: [D26](#d26).

<a id="d29"></a>

### D29. GPU resources и residency

Модули: [src/gpu.rs](../src/gpu.rs), [src/gpu_regex.rs](../src/gpu_regex.rs), [tests/gpu.rs](../tests/gpu.rs). 40 пунктов: `LIN-1121`–`LIN-1160`.

Контекст: [D18](#d18).

<a id="d30"></a>

### D30. GPU kernels и equivalence

Модули: [src/gpu.rs](../src/gpu.rs), [src/shaders/compare.wgsl](../src/shaders/compare.wgsl), [tests/shaders.rs](../tests/shaders.rs). 40 пунктов: `LIN-1161`–`LIN-1200`.

Контекст: [D29](#d29).

<a id="d31"></a>

### D31. Graph queries

Модули: [src/graph.rs](../src/graph.rs), [src/store.rs](../src/store.rs), [src/cursor.rs](../src/cursor.rs). 40 пунктов: `LIN-1201`–`LIN-1240`.

Контекст: [D08](#d08), [D06](#d06).

<a id="d32"></a>

### D32. Cold storage и mmap

Модули: [src/cold.rs](../src/cold.rs), [src/store.rs](../src/store.rs), [src/persist.rs](../src/persist.rs). 40 пунктов: `LIN-1241`–`LIN-1280`.

Контекст: [D04](#d04), [D05](#d05).

<a id="d33"></a>

### D33. Async execution и backpressure

Модули: [src/async_db.rs](../src/async_db.rs), [src/query.rs](../src/query.rs), [tests/query.rs](../tests/query.rs). 40 пунктов: `LIN-1281`–`LIN-1320`.

Контекст: [D16](#d16), [D34](#d34).

<a id="d34"></a>

### D34. Concurrency и read snapshots

Модули: [src/exec.rs](../src/exec.rs), [src/store.rs](../src/store.rs), [src/async_db.rs](../src/async_db.rs). 40 пунктов: `LIN-1321`–`LIN-1360`.

Контекст: [D06](#d06).

<a id="d35"></a>

### D35. Replication transport

Модули: [src/ship.rs](../src/ship.rs), [src/main.rs](../src/main.rs), [tests/persist.rs](../tests/persist.rs). 40 пунктов: `LIN-1361`–`LIN-1400`.

Контекст: [D02](#d02), [D04](#d04).

<a id="d36"></a>

### D36. Followers и catch-up

Модули: [src/ship.rs](../src/ship.rs), [src/persist.rs](../src/persist.rs), [src/main.rs](../src/main.rs). 40 пунктов: `LIN-1401`–`LIN-1440`.

Контекст: [D35](#d35).

<a id="d37"></a>

### D37. Public Rust API

Модули: [src/lib.rs](../src/lib.rs), [src/query.rs](../src/query.rs), [src/exec.rs](../src/exec.rs). 40 пунктов: `LIN-1441`–`LIN-1480`.

Контекст: [D13](#d13).

<a id="d38"></a>

### D38. Typed queries и derive

Модули: [lin-derive/src/lib.rs](../lin-derive/src/lib.rs), [src/row.rs](../src/row.rs), [src/query.rs](../src/query.rs). 40 пунктов: `LIN-1481`–`LIN-1520`.

Контекст: [D37](#d37).

<a id="d39"></a>

### D39. CLI и интерактивность

Модули: [src/main.rs](../src/main.rs), [src/explain.rs](../src/explain.rs), [tests/compute_cli.rs](../tests/compute_cli.rs). 40 пунктов: `LIN-1521`–`LIN-1560`.

Контекст: [D37](#d37).

<a id="d40"></a>

### D40. Errors и диагностика

Модули: [src/error.rs](../src/error.rs), [src/parse.rs](../src/parse.rs), [src/check.rs](../src/check.rs). 40 пунктов: `LIN-1561`–`LIN-1600`.

Контекст: [D13](#d13).

<a id="d41"></a>

### D41. Observability

Модули: [src/exec.rs](../src/exec.rs), [src/persist.rs](../src/persist.rs), [src/explain.rs](../src/explain.rs). 40 пунктов: `LIN-1601`–`LIN-1640`.

Контекст: [D40](#d40).

<a id="d42"></a>

### D42. Property и differential testing

Модули: [tests/lang.rs](../tests/lang.rs), [tests/query.rs](../tests/query.rs), [tests/persist.rs](../tests/persist.rs). 40 пунктов: `LIN-1641`–`LIN-1680`.

Контекст: [D13](#d13).

<a id="d43"></a>

### D43. Crash и fault injection

Модули: [src/persist.rs](../src/persist.rs), [src/ship.rs](../src/ship.rs), [tests/persist.rs](../tests/persist.rs). 40 пунктов: `LIN-1681`–`LIN-1720`.

Контекст: [D02](#d02), [D03](#d03), [D04](#d04).

<a id="d44"></a>

### D44. CI и платформы

Модули: [.github/workflows/bench.yml](../.github/workflows/bench.yml), [scripts/ci-bench.sh](../scripts/ci-bench.sh), [tests/shaders.rs](../tests/shaders.rs). 40 пунктов: `LIN-1721`–`LIN-1760`.

Контекст: [D01](#d01), [D42](#d42).

<a id="d45"></a>

### D45. Безопасность и resource limits

Модули: [src/exec.rs](../src/exec.rs), [src/persist.rs](../src/persist.rs), [src/ship.rs](../src/ship.rs). 40 пунктов: `LIN-1761`–`LIN-1800`.

Контекст: [D40](#d40).

<a id="d46"></a>

### D46. Catalog и schema evolution

Модули: [src/catalog.rs](../src/catalog.rs), [src/check.rs](../src/check.rs), [src/exec.rs](../src/exec.rs). 40 пунктов: `LIN-1801`–`LIN-1840`.

Контекст: [D06](#d06).

<a id="d47"></a>

### D47. Import и export

Модули: [src/main.rs](../src/main.rs), [src/row.rs](../src/row.rs), [src/batch.rs](../src/batch.rs). 40 пунктов: `LIN-1841`–`LIN-1880`.

Контекст: [D18](#d18), [D46](#d46).

<a id="d48"></a>

### D48. Operations и lifecycle

Модули: [src/main.rs](../src/main.rs), [src/persist.rs](../src/persist.rs), [src/ship.rs](../src/ship.rs). 40 пунктов: `LIN-1881`–`LIN-1920`.

Контекст: [D03](#d03), [D04](#d04), [D05](#d05).

<a id="d49"></a>

### D49. Build и distribution

Модули: [Cargo.toml](../Cargo.toml), [Cargo.lock](../Cargo.lock), [src/lib.rs](../src/lib.rs). 40 пунктов: `LIN-1921`–`LIN-1960`.

Контекст: [D44](#d44).

<a id="d50"></a>

### D50. Документация и продуктовый контракт

Модули: [README.md](../README.md), [CHANGELOG.md](../CHANGELOG.md), [docs/benchmark-contract.md](../docs/benchmark-contract.md). 40 пунктов: `LIN-1961`–`LIN-2000`.

Контекст: [D01](#d01), [D37](#d37).

## Явные зависимости отдельных задач

- `LIN-0001` → `LIN-0029`.
- `LIN-0002` → `LIN-0081`, `LIN-0082`, `LIN-0084`.
- `LIN-0003` → `LIN-0081`, `LIN-0082`, `LIN-0084`.
- `LIN-0004` → `LIN-0029`.

## Происхождение и контроль полноты

Основа: текущие модули и история коммитов, [контракт бенчмарков](benchmark-contract.md), [история измерений](benchmark-history.md), [текущий baseline](../benches/results/2026-10-21-current-native-durable/report.md), [compute qualification](compute-support.md). Старый RAG-каталог `4d4d3e41-29ff-4236-8c12-8624850067e6` использован только как источник направлений: его пропущенные диапазоны и широкие заявления не переносятся как выполненные проверки или готовые задачи.

Машиночитаемый источник — [roadmap-2000.json](roadmap-2000.json). Проверка: `python3 scripts/check-roadmap-2000.py`. Она проверяет количество, уникальность ID/названий, ранги, области, зависимости, gate, существование файлов и соответствие Markdown JSON. Она не доказывает актуальность каждой гипотезы или наличие каждого предполагаемого дефекта; это предмет triage.

Итоговые количества: P1: 110, P2: 1290, P3: 600; статусы: blocked_external: 2, candidate: 989, measured_gap: 3, qualification: 1006.
