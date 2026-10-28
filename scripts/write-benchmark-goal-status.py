#!/usr/bin/env python3
"""Render source-scoped benchmark gaps from retained raw summaries, without rerunning engines."""
from pathlib import Path
import collections
import json

root = Path(__file__).resolve().parents[1]
def read(relative):
    return json.loads((root / relative).read_text())
base = 'benches/results/2026-10-27-durable-mutation-baseline-linux'
summary = read(base + '/summary.json')
qualification = read(base + '/qualification.json')
provenance = read(base + '/provenance.json')
assert qualification['measurement_complete'] and qualification['comparisons'] == 384
rows = summary['comparisons']
assert len(rows) == 64 and sum(r['lin_wins'] for r in rows) == qualification['lin_wins'] == 192
for peer, wins in [('sqlite_delete', 192), ('sqlite_wal', 0)]:
    assert sum(r['lin_wins'] for r in rows if r['peer'] == peer) == wins
peer_path = 'benches/results/2026-10-27-corrected-peer-refresh/audit.json'
peer = read(peer_path)
reads = next(r for r in peer['results'] if r['mode'] == 'read')['comparisons']
assert len(reads) == 108 and all(r['positive_processes'] == 3 for r in reads)
by_peer = collections.defaultdict(list)
for row in reads:
    by_peer[row['peer']].append(row)
assert set(by_peer) == {'sqlite', 'duckdb', 'postgres', 'mysql', 'mongo', 'pandas'}
lines = ['# Прогресс к превосходству над восемью движками — 2026-10-27', '',
    'Это сводка выбранных завершённых измерений и открытых проверок, а не новый бенчмарк. Результаты привязаны к исходникам и контрактам; выигрыши старого бинарника не переносятся на последующие исправления.', '',
    '## Основной измеренный разрыв: SQLite FULL WAL', '',
    f"Исторический source commit `{provenance['source_commit']}`; core соответствует производственному `04749994b9ca7a090476b95401f0f7166d8b870a`. Сводка: [полный отчёт](../{base}/report.md). 576 процессов, 4608 наблюдений, две серии по шесть повторов и два A/A-ролевых запуска одного бинарника. В сравнении с SQLite FULL DELETE — 192/192 числовых побед; с FULL WAL — 0/192. В обеих СУБД сохраняется Full-sync.", '',
    'Измерение выполнено **до исправления поколений/атомарной публикации интерактивных транзакций**. Оно указывает историческое направление работы, но не аттестует исправленный бинарник. Rollback не делает durable commit; его нельзя смешивать с latency успешной записи.', '',
    '| Операция | Размер | Lin/SQLite WAL, диапазон четырёх медиан | Снижение latency Lin до равенства, % | Победы |',
    '|---|---:|---:|---:|---:|']
for size in ['1k', '10k']:
    for op in ['update', 'delete', 'commit', 'rollback']:
        case = f'compare/durable_{op}_1row_{size}/lin'
        group = [r for r in rows if r['case'] == case and r['peer'] == 'sqlite_wal']
        assert len(group) == 4 and all(r['pairs'] == 6 for r in group)
        ratios = [r['median_ratio'] for r in group]
        assert all(r > 1 for r in ratios)
        cuts = [100 * (1 - 1 / r) for r in ratios]
        lines.append(f"| {op} | {size} | {min(ratios):.3f}–{max(ratios):.3f} | {min(cuts):.1f}–{max(cuts):.1f} | {sum(r['lin_wins'] for r in group)}/24 |")
lines += ['', 'Необходимое снижение вычисляется как `100 × (1 − 1/ratio)`, при фиксированной скорости SQLite. Для победы требуется снижение больше границы равенства и новый независимый замер; это не прогноз ускорения.', '',
    'В [отдельном caller-thread профиле](../benches/results/2026-10-27-durable-mutation-phase-engine-linux/report.md) sync занимает примерно 82–86% измеренного времени успешно фиксируемых операций; payload — 4112 байт, одна запись и один sync. Это диагностический другой прогон с собственной погрешностью наблюдателя. Отсюда приоритет проверки реального I/O; процент профиля не является точной долей времени другого бинарника.', '',
    '## Что проверено относительно каждого peer', '',
    f"[Свежий API read/native refresh](../benches/results/2026-10-27-corrected-peer-refresh/report.md) относится к `{peer['source_commit']}`: 1k/10k/100k, три процесса, подготовленные запросы и полный readback. Минимумы ниже — среди сохранённых сравнений чтения этого прогона.", '',
    '| Peer | Сравнения чтения | Положительные процессы | Минимальное отношение peer/Lin | Незакрытая часть |',
    '|---|---:|---:|---:|---|']
for name in ['duckdb', 'sqlite', 'postgres', 'mysql', 'mongo', 'pandas']:
    group = by_peer[name]
    assert len(group) == 18
    gap = 'Durable/native mutation gaps и повтор на итоговом бинарнике' if name == 'sqlite' else 'Повтор на итоговом бинарнике; расширение согласованных сценариев'
    if name == 'pandas': gap = 'Только in-memory read API; durable-транзакции неприменимы'
    lines.append(f"| {name} | {len(group)}/18 | {sum(r['positive_processes'] for r in group)}/54 | {min(r['speedup'] for r in group):.3f}× | {gap} |")
lines += [
    '| MSSQL | Отдельный реальный native x86 CI | Функциональная квалификация завершена | Публично не раскрывается | Метрики приватны; green CI не означает победу |',
    '| Kusto | Отсутствуют в выбранных завершённых измерениях | Не квалифицирован | — | Нужен реальный разрешённый endpoint и замер |', '',
    '[MSSQL qualification](../benches/results/2026-10-26-mssql-native-ci/report.md) относится к frozen `8b0e9717`, не к текущим исходникам; comparative evidence остаётся приватным. Сравнение API включает Python bindings у embedded peers и driver/network у серверов, а не только CPU движков. Native insertion в API refresh использует разные объявленные durability-контракты и tmpfs у серверов: 15/15 агрегированных побед,45/45 процессных в свежем прогоне. В [предыдущем source-scoped прогоне](../benches/results/2026-10-26-persistent-peer-refresh/report.md) был проигрыш DuckDB на100k:он сохранён и не объявляется устранённым причинным A/B-изменением. Это не замена Full durable gate.', '',
    '## Текущие независимые проверки', '',
    '- [WAL generation/atomic frame repair](../benches/results/2026-10-27-tx-wal-generation/report.md): rollback, один frame на commit, checkpoint error и выход дочернего процесса без Drop. Локально 8 регрессий; [Linux: 415 passed /2 ignored, независимый аудит](../benches/results/2026-10-27-tx-wal-generation-linux/report.md).',
    '- [Normal-mode multi-write diagnostic](../benches/results/2026-10-27-tx-wal-generation-linux/report.md): существующий test timer напечатал Lin 161444 ns/tx против SQLite WAL NORMAL 57176 ns/tx на пяти записях за транзакцию. Один процесс без A/A; проигрыш сохранён, отдельная парная квалификация отсутствует. Текущая Full-sync one-row матрица этот контракт не закрывает.',
    '- [Abort после ошибки транзакции](../benches/results/2026-10-27-tx-quota-abort/report.md): quota/CAS/parse error не позволяют commit; локально полный набор 420 passed /2 ignored; [Linux:420 passed /2 ignored, независимый аудит](../benches/results/2026-10-27-tx-quota-abort-linux/report.md). Это более новый exec, он не включён в frozen WAL timing input.',
    '- [Bounded Linux KEEP_SIZE](../benches/results/2026-10-27-linux-wal-keep-size-tx-fixed/report.md): исправленный независимый CRC32C-эталон и прежние replay/follower/checkpoint checks; локально 417 passed /2 ignored; [Linux:421 passed /2 ignored, независимый аудит](../benches/results/2026-10-27-linux-wal-keep-size-tx-fixed-linux/report.md).',
    '- [Normal checkpoint concentration diagnostic](../benches/results/2026-10-27-normal-tx-checkpoint-diagnostic/report.md): на macOS,6 свежих процессов,6000 наблюдений;31 checkpoint на1000 транзакций Lin,92–95% суммарного времени в этих транзакциях. Политики32 commit/1000 WAL pages не менялись. Изолированные sync/serialization этапы и Linux-скорость этим не измерены.',
    '- [Normal checkpoint stage diagnostic](../benches/results/2026-10-27-normal-checkpoint-stages/report.md):15 свежих процессов,15000 транзакций,93 автоматических checkpoint-кадра; составные этапы snapshot ≈33%,HEAD ≈31%,compact ≈25%,FTS ≈6%. macOS,три повтора; исходный/A/A/выключенный/включённый наблюдатель сохранены. Изолированная цена syscalls и ускорение кандидата не доказаны.',
    '- [Schema crash/HEAD recovery](../benches/results/2026-10-27-schema-crash-head/report.md):каталог HEAD сверяется на соответствующем поколении;порядок публикации и sync не менялись. [Linux:430 passed /2 ignored,10 целевых регрессий](../benches/results/2026-10-27-schema-crash-head-linux/report.md). Данная более новая store-реализация не входит в frozen WAL timing input.',
    '- [Full-engine Linux WAL reserve: rejected](../benches/results/2026-10-27-linux-wal-reserve-engine-linux/report.md): all 2196 processes /17568 observations completed and independently audited; 10/12 target windows and 8/110 control windows failed. SQLite FULL DELETE 96/96 wins; SQLite FULL WAL 0/96. No production enablement or unchanged rerun. Frozen6d518a8a only; newer store/exec changes are outside this experiment.',
    '- [Незавершённый lazy COW](../benches/results/2026-10-27-cow-lazy-sharing-engine-linux-incomplete/report.md): отменённый run; 1207 status-файлов из 1332 требуемых процессов, итоговый gate не выполнялся. Принятого ускорения нет; публичный Row нового типа не включён в production.', '',
    'Состояние выполнения CI проверяется по самим run: этот документ фиксирует объём и ссылки, а не предсказывает окончательный статус. Для принятия WAL-кандидата нужны все целевые пары и все контроли; затем требуется повтор затронутых контрактов на итоговом исправленном бинарнике. Полная цель по всем восьми peers **не достигнута**.', '',
    '## Воспроизведение сводки', '',
    '`python3 scripts/write-benchmark-goal-status.py` читает сохранённые JSON и проверяет количества/источники. Он не запускает СУБД и не заменяет независимый raw audit исходных экспериментов.']
# Keep newer experiments separate from the historical production baseline above.
zero = json.loads((root / 'benches/results/2026-10-27-wal-zero-full-engine-linux/independent-audit.json').read_text())
warm = json.loads((root / 'benches/results/2026-10-27-incremental-owned-rollback/summary.json').read_text())
alloc = json.loads((root / 'benches/results/2026-10-27-incremental-rollback-allocation/summary.json').read_text())
assert zero['audit_complete'] and zero['processes'] == 2268 and zero['observations'] == 18144
assert not zero['source_candidate_qualified'] and zero['failed_target_windows'] == 9 and zero['failed_control_windows'] == 12
assert zero['candidate_peer_counts']['sqlite_wal'] == dict(wins=18, comparisons=96)
assert warm['diagnostic_gate_passes'] and not warm['full_engine_qualified'] and warm['processes'] == 36 and warm['observations'] == 3456
rollback = json.loads((root / 'benches/results/2026-10-28-incremental-rollback-engine-linux/independent-audit.json').read_text())
assert rollback['audit_complete'] and rollback['processes'] == 2412 and rollback['observations'] == 19296
assert not rollback['source_candidate_qualified'] and rollback['failed_target_windows'] == 0 and rollback['failed_control_windows'] == 14
assert rollback['candidate_peer_counts']['sqlite_wal'] == dict(wins=8, comparisons=96)
direct_linux = json.loads((root / 'benches/results/2026-10-28-wal-direct-batch-full-engine-linux/independent-audit.json').read_text())
assert direct_linux['audit_complete'] and direct_linux['processes'] == 2268 and direct_linux['observations'] == 18144
assert not direct_linux['source_candidate_qualified'] and direct_linux['failed_target_windows'] == 2 and direct_linux['failed_control_windows'] == 8
assert direct_linux['cpu_component_gate_passes'] and direct_linux['candidate_peer_counts']['sqlite_wal'] == dict(wins=11, comparisons=96)
batch = json.loads((root / 'benches/results/2026-10-28-wal-direct-batch/cpu-summary.json').read_text())
assert batch['cpu_gate_passes'] and not batch['full_engine_qualified'] and not batch['full_objective_achieved']
assert alloc['allocation_audit_passed'] and alloc['processes'] == 6 and alloc['windows'] == 216 and not alloc['latency_qualified'] and not alloc['peak_rss_qualified']
lines += [
    '', '## Дополнительные эксперименты: после исходной сводки', '',
    'Перечисленные ниже новые данные не переаттестуют старые бинарники и не означают, что прототипы включены в main.', '',
    '- [Positive-zero WAL, полный Linux gate: rejected](../benches/results/2026-10-27-wal-zero-full-engine-linux/report.md):2268 процессов/18144 наблюдения, независимый raw/source audit прошёл. Провалены9/12 целевых и12/114 контрольных окон; ни одна цель не прошла обе серии. SQLite FULL WAL18/96, FULL DELETE95/96; принятое ускорение отсутствует.',
    '- [Outlined positive-zero CPU: rejected](../benches/results/2026-10-27-wal-positive-zero-outlined/report.md):1296000 кодирований; UPDATE/DELETE выигрывают около21–22%, NoVec проигрывает около4% в обеих сериях. Дорогой новый full-engine прогон этой версии не запускался.',
    '- [Single-buffer canonical WAL: stable win not established](../benches/results/2026-10-27-wal-single-buffer/report.md):433 passed/2 ignored;3456 операций encode+CRC+write+sync с точным frame readback. Для обычного вектора Full −1.129%/−3.142%; все четыре целевых окна не прошли. Это macOS component evidence, не Linux DB transaction.',
    '- [Physical WAL padding: несовместимость torn-tail recovery](../benches/results/2026-10-27-wal-physical-tail-feasibility/report.md):один и тот же оборванный frame с обычным EOF восстанавливается, с хвостом нулей возвращает corruption. Полный padded WAL читается. Простой set_len не принят; corruption policy не ослаблялась.',
    '- [Schema WAL rejection correctness repair](../benches/results/2026-10-27-schema-wal-rejection/report.md):до исправления отклонённая schema-запись оставляла неверный Persist.catalog_hash и мешала reopen после успешного close. Hash публикуется после успешного append;431 passed/2 ignored. Это исправление корректности, не latency-ускорение.',
    '- [Clone-free owned rollback: gate failed](../benches/results/2026-10-27-clone-free-owned-rollback/report.md):медианы rollback +22–25%, UPDATE-контроли прошли, но одна10k-пара −12.760% и критерий6/6 не выполнен. Потерянная пара сохранена; одинаковый прогон не повторялся ради pass.',
    '- [Incremental owned rollback: локальный warm gate passed](../benches/results/2026-10-27-incremental-owned-rollback/report.md):434 passed/2 ignored;36 процессов/3456 наблюдений. Rollback +42–47%, все6целевых окон6/6 выше A/A, UPDATE-контроли прошли. Точный Row=BTreeMap, полный payload, sync и квоты сохранены; это ещё не whole-engine/peer acceptance.',
    '- [Allocation requests](../benches/results/2026-10-27-incremental-rollback-allocation/report.md):6 процессов/216 окон; UPDATE+rollback154–158→72 allocations,9922–11130→5356 запрошенных байт. UPDATE-пара141 allocations/9848байт совпадает точно. Не RSS, не retained heap и не дополнительное измерение latency.',
    '- [Fallback и wrong-order mutant](../benches/results/2026-10-27-incremental-rollback-fallback/report.md):3положительных теста; неверный порядок rebuild ловится2тестами. [FTS fold stress](../benches/results/2026-10-27-incremental-rollback-fts-fold/report.md):1200rollback,400unique failures,110Mixed undo,12000сравнений с перестроенным FTS; полные итоговые строки совпали.',
    '- [Incremental rollback: полный Linux gate rejected](../benches/results/2026-10-28-incremental-rollback-engine-linux/report.md):67сценариев,2412процессов/19296наблюдений. Все10целевых окон прошли, но14/124контрольных окон провалены; source gate false. SQLite FULL WAL8/96, FULL DELETE96/96; неизменённый кандидат не принят и повторно не запускался.',
]
lines += [
    '', '## Новый прямой MessagePack encoder — 2026-10-28', '',
    '- [UPDATE/DELETE-only версия](../benches/results/2026-10-28-wal-direct-msgpack/decision.md):CPU gate passed,437tests/2ignored; полный timing не запускался, потому что интерактивный commit использует Batch с canonical fallback.',
    '- [Direct Batch](../benches/results/2026-10-28-wal-direct-batch/report.md):CPU gate passed,12процессов/396блоков/1584000кодирований;single-UPDATE Batch51.618%/51.333%,mixed Batch50.055%/50.904%,все контроли прошли.439tests/2ignored;negative-zero mutant caught. Это сериализация, не commit latency.',
    '- [Старый reader](../benches/results/2026-10-28-wal-direct-batch-legacy-reader/report.md):5фаз,полные строки/поколения,IEEE-биты;повреждённый frame и подмена ожидаемых строк обнаружены.',
    '- [Frozen full-engine protocol](../benches/results/2026-10-28-wal-direct-batch-full-engine/protocol.md),[run40577759103](https://github.com/themoretheless/lin/actions/runs/40577759103):source696b692a7,63сценария/2268процессов/18144наблюдения.126локальных smoke прошли. [Полный Linux gate rejected](../benches/results/2026-10-28-wal-direct-batch-full-engine-linux/report.md):2/12target и8/114control окон провалены; SQLite FULL WAL11/96. Raw/source audit прошёл; принятого ускорения нет.',
    '- Замороженный генератор run40577759103 содержит старый заголовок и описательный CPU-флаг false. Это ошибка метаданных, не latency-порогов. scripts/audit-wal-direct-batch-engine.py проверяет фактический CPU gate отдельно по точному candidate hash и сохраняет исходные JSON без изменения. Независимый аудит полного Linux raw выполнен и прошёл; gate всё равно false.',
]
cow = read('benches/results/2026-10-28-cow-vnext-full-engine-linux/independent-audit.json')
assert cow['audit_complete'] and cow['processes'] == 2412 and cow['observations'] == 19296
assert cow['failed_target_windows'] == 0 and cow['failed_control_windows'] == 11
assert cow['source_candidate_qualified'] is False
assert cow['candidate_peer_counts']['sqlite_wal'] == {'wins': 3, 'comparisons': 96}
lines += [
    '', '## COW следующей версии API — разрешённое развитие', '',
    'Пользователь явно выбрал развитие COW и отдельного публичного Row для следующей версии API. [Миграция и scope](cow-row-vnext.md). Это разрешение на API-переход, не отмена correctness/performance gate.',
    '- [Порт актуальных исправлений](../benches/results/2026-10-28-cow-vnext-port/report.md):443debugtests/2ignored;all-targets dev check прошёл. Ни одна старая метрика не переносится автоматически.',
    '- [Свежий warm COW vs BTreeMap](../benches/results/2026-10-28-cow-vnext-warm/report.md):36процессов/3456наблюдений;UPDATE57.624–61.046%,все6окон6/6aboveA/A. Общий diagnostic gate false:rollback10k/series1 имел5/6;пара сохранена.',
    '- [Owned undo rejected](../benches/results/2026-10-28-cow-owned-undo-warm/report.md) и [Pack-before-undo rejected](../benches/results/2026-10-28-cow-pack-before-undo-warm/report.md):обе гипотезы измерены и отклонены,исходники восстановлены. Held-result regression сохранён; полная квалификация COW по native/durable/read/peer контрактам ещё требуется.',
    '- [Владение Pack](../benches/results/2026-10-28-cow-rollback-owners/report.md):runtimecounts подтверждают дополнительного владельца Row;это не доказательство причины замедления или выигрыша latency.',
    '- [Полная COW-квалификация](../benches/results/2026-10-28-cow-vnext-full-engine/protocol.md),[run40588890401](https://github.com/themoretheless/lin/actions/runs/40588890401):source00c8aa9a3,67cases/2412processes/19296observations;3nativeUPDATEtargets,64контроля.444release tests/2ignored,134localsmokes и synthetic sensitivity прошли. [Полный независимо аттестованный результат](../benches/results/2026-10-28-cow-vnext-full-engine-linux/audited-report.md):все6nativeUPDATEокон прошли,выигрыш15.599–21.570%;11контрольных окон провалены,включая3скалярныхWALLin. Sourcegate false;SQLiteFULLWAL3/96,SQLiteDELETE96/96,native/readSQLite67/228,DuckDB36/36. Полный архив15183файлов проверен поSHA256. Не включено в production; повтор неизменённого кандидата не оправдан.',
]
small = read('benches/results/2026-10-28-cow-small-row-inline/independent-audit.json')
index = read('benches/results/2026-10-28-index-single-reverse-write/independent-audit.json')
assert small['audit_complete'] and small['processes'] == 144 and small['observations'] == 1152
assert small['failed_target_windows'] == 4 and small['failed_control_windows'] == 0
assert index['audit_complete'] and index['processes'] == 36 and index['observations'] == 2304
assert index['failed_target_windows'] == 5 and index['failed_control_windows'] == 0
phase_smokes = read('benches/results/2026-10-28-cow-durable-phase-refresh/smoke-status.json')
phase_tests = read('benches/results/2026-10-28-cow-durable-phase-refresh/test-status.json')
assert len(phase_smokes) == 30 and all(row['exit_code'] == 0 for row in phase_smokes)
assert phase_tests['exit_code'] == 0 and phase_tests['passed'] == 445 and phase_tests['source_restored']
lines += [
    '- [Двухполевой inline rejected](../benches/results/2026-10-28-cow-small-row-inline/report.md):144процесса/1152наблюдения;4/4скалярныхWALtarget окна провалены,контроли прошли. A/A9–31%;устойчивое ускорение не установлено. Прежний COW восстановлен.',
    '- [Однократная reverse-map запись rejected](../benches/results/2026-10-28-index-single-reverse-write/report.md):36процессов/2304warmCPU наблюдения;5/8target окон смены ключа провалены,всеunchanged-key контроли прошли. Медианы3–26% не прошли frozen gate. Прежний индекс восстановлен;этот probe не измеряет полный UPDATE/WAL.',
    '- [Профиль текущего COW](../benches/results/2026-10-28-cow-durable-phase-refresh/local-qualification.md):445release tests/2ignored инструментированного кода и30локальных full-value/reopen/phase-count smokes прошли;исходники восстановлены. [Linux run40602697359](https://github.com/themoretheless/lin/actions/runs/40602697359),source75f8f6b7f:240процессов/1920wall наблюдений с original/control/disabled/traced. [Завершённый raw/source audit](../benches/results/2026-10-28-cow-durable-phase-refresh-linux/audited-report.md) прошёл;векторный Full sync80.986–87.031%,скалярный Normal exec97.923%. Это фазы инструментария на source75f8,не аттестация нового кандидата или точные доли другого бинарника.',
]
phase_audit = read('benches/results/2026-10-28-cow-durable-phase-refresh-linux/frozen-audit.json')
assert phase_audit['audit_complete'] and phase_audit['processes'] == 240 and phase_audit['observations'] == 1920
cas = read('benches/results/2026-10-28-single-stmt-cas-snapshot/independent-audit.json')
assert cas['audit_complete'] and cas['component_gate_passes'] and cas['processes'] == 144 and cas['observations'] == 1152
assert cas['failed_target_windows'] == cas['failed_control_windows'] == 0
full_prerequisites = read('benches/results/2026-10-28-cow-cas-full-engine/local-prerequisite-status.json')
assert full_prerequisites['release_tests_passed'] == 449 and full_prerequisites['correctness_smokes_passed'] == 134
assert full_prerequisites['cow_eq_mutant_runtime_rejected'] and full_prerequisites['multi_statement_cas_mutant_runtime_rejected']
full_audit = read('benches/results/2026-10-28-cow-cas-full-engine-linux/independent-audit.json')
assert full_audit['audit_complete'] and full_audit['processes'] == 2412 and full_audit['observations'] == 19296
assert not full_audit['source_candidate_qualified'] and full_audit['failed_target_windows'] == 0 and full_audit['failed_control_windows'] == 7
rwf = read('benches/results/2026-10-28-wal-rwf-dsync-probe-linux/frozen-audit.json')
assert rwf['audit_complete'] and rwf['processes'] == 432 and rwf['observations'] == 3456
assert not rwf['io_gate_passes'] and rwf['failed_full_windows'] == 16 and rwf['failed_control_windows'] == 1
mssql = read('benches/results/2026-10-28-mssql-current-cow/functional-status.json')
assert mssql['functional_raw_audit_complete'] and mssql['frozen_source_hashes_verified'] and mssql['owned_container_cleanup_verified']
assert mssql['comparative_metrics_public'] is False and mssql['full_objective_achieved'] is False
lines += [
    '- [Однооператорный CAS snapshot component gate passed](../benches/results/2026-10-28-single-stmt-cas-snapshot/report.md):source2633067cb,144процесса/1152наблюдения;Full31.932–35.768%,Normal91.371–91.663%,все4target окна6/6aboveA/A. Контроли прошли порог;native100k медианы были отрицательны в пределахA/A,поэтому нулевая регрессия не доказана.449release tests/2ignored;неверный multi-statement bypass пойман runtime-тестом. Полная матрица ещё нужна.',
    '- [Совместный COW+CAS full-source gate failed](../benches/results/2026-10-28-cow-cas-full-engine-linux/decision.md),[run40607797376](https://github.com/themoretheless/lin/actions/runs/40607797376):source36666994a,все2412процессов/19296наблюдений проверены независимым аудитом. Все10target окон прошли6/6aboveA/A: native UPDATE13.941–20.267%,scalar Full52.036–52.999%,Normal94.651–94.680%. Семь control окон провалены: Lin bulk INSERT10k в обеих сериях,filter_eq,durable UPDATE1k и три неизменённых SQLite окна. Peer wins: SQLite DELETE96/96,WAL3/96,native/read49/228,DuckDB36/36. Кандидат не принят;производственная и полная восьми-peer квалификация отсутствуют.',
    '- [RWF_DSYNC I/O probe rejected](../benches/results/2026-10-28-wal-rwf-dsync-probe-linux/report.md):run40614973547,432процесса/3456наблюдений;все16Fulltarget окон и1Normalcontrol провалены. Медианы Full от-1.435%до+3.112%,устойчивого ускорения нет.24Linuxsmoke и4error-path теста прошли,raw/source audit выполнен. Производственный код и WAL не менялись;интеграция не выполняется.',
    '- [MSSQL private refresh](../benches/results/2026-10-28-mssql-current-cow/report.md),[run40603880905](https://github.com/themoretheless/lin/actions/runs/40603880905):замороженный предшествующий COW source75f8f6b7f;native-x86 стенд завершён,локальный raw/source audit и cleanup прошли. В репозитории только encrypted archive и функциональный статус;относительные исходы приватны. Это не публичное доказательство выигрыша и не квалификация нового CAS-кандидата.',
]
output = root / 'docs/benchmark-goal-status-2026-10-27.md'
output.write_text('\n'.join(lines) + '\n')
# Never expose endpoint values or credentials in the status artifact.
print(f'Wrote {output.relative_to(root)}; historical 384 mutation comparisons and 108 read comparisons; objective remains open')
