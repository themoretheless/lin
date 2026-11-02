#!/usr/bin/env python3
"""Render source-scoped benchmark gaps from retained raw summaries, without rerunning engines."""
from pathlib import Path
import collections
import json
import subprocess

root = Path(__file__).resolve().parents[1]
def read(relative):
    path = root / relative
    # Sparse worktrees can omit retained artifacts; use their exact committed blobs.
    data = path.read_bytes() if path.is_file() else subprocess.check_output(
        ['git', 'show', 'HEAD:' + relative], cwd=root)
    return json.loads(data)
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
zero = read('benches/results/2026-10-27-wal-zero-full-engine-linux/independent-audit.json')
warm = read('benches/results/2026-10-27-incremental-owned-rollback/summary.json')
alloc = read('benches/results/2026-10-27-incremental-rollback-allocation/summary.json')
assert zero['audit_complete'] and zero['processes'] == 2268 and zero['observations'] == 18144
assert not zero['source_candidate_qualified'] and zero['failed_target_windows'] == 9 and zero['failed_control_windows'] == 12
assert zero['candidate_peer_counts']['sqlite_wal'] == dict(wins=18, comparisons=96)
assert warm['diagnostic_gate_passes'] and not warm['full_engine_qualified'] and warm['processes'] == 36 and warm['observations'] == 3456
rollback = read('benches/results/2026-10-28-incremental-rollback-engine-linux/independent-audit.json')
assert rollback['audit_complete'] and rollback['processes'] == 2412 and rollback['observations'] == 19296
assert not rollback['source_candidate_qualified'] and rollback['failed_target_windows'] == 0 and rollback['failed_control_windows'] == 14
assert rollback['candidate_peer_counts']['sqlite_wal'] == dict(wins=8, comparisons=96)
direct_linux = read('benches/results/2026-10-28-wal-direct-batch-full-engine-linux/independent-audit.json')
assert direct_linux['audit_complete'] and direct_linux['processes'] == 2268 and direct_linux['observations'] == 18144
assert not direct_linux['source_candidate_qualified'] and direct_linux['failed_target_windows'] == 2 and direct_linux['failed_control_windows'] == 8
assert direct_linux['cpu_component_gate_passes'] and direct_linux['candidate_peer_counts']['sqlite_wal'] == dict(wins=11, comparisons=96)
batch = read('benches/results/2026-10-28-wal-direct-batch/cpu-summary.json')
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
soa = read('benches/results/2026-10-28-cow-soa-text-reuse-component-linux/frozen-audit.json')
assert soa['audit_complete'] and soa['processes'] == 324 and soa['observations'] == 2592
assert not soa['component_gate_passes'] and soa['failed_target_windows'] == 5 and soa['failed_control_windows'] == 2
bulk_template = read('benches/results/2026-10-28-cow-bulk-map-template-component-linux/frozen-audit.json')
assert bulk_template['audit_complete'] and bulk_template['processes'] == 288 and bulk_template['observations'] == 2304
assert not bulk_template['component_gate_passes'] and bulk_template['failed_target_windows'] == 4 and bulk_template['failed_control_windows'] == 2
cmp_coverage = read('benches/results/2026-10-28-index-single-cmp-coverage-component-linux/frozen-audit.json')
assert cmp_coverage['audit_complete'] and cmp_coverage['processes'] == 288 and cmp_coverage['observations'] == 2304
assert not cmp_coverage['component_gate_passes'] and cmp_coverage['failed_target_windows'] == 1 and cmp_coverage['failed_control_windows'] == 1
delete_keys = read('benches/results/2026-10-28-delete-key-wal-component-linux/frozen-audit.json')
assert delete_keys['audit_complete'] and delete_keys['processes'] == 288 and delete_keys['observations'] == 2304
assert not delete_keys['component_gate_passes'] and delete_keys['failed_target_windows'] == 2 and delete_keys['failed_control_windows'] == 1
assert delete_keys['production_golden_cases'] == 8 and delete_keys['quota_bypass_runtime_rejected']
noalloc_coverage = read('benches/results/2026-10-28-index-allocation-free-coverage-component-linux/frozen-audit.json')
assert noalloc_coverage['audit_complete'] and noalloc_coverage['processes'] == 288 and noalloc_coverage['observations'] == 2304
assert not noalloc_coverage['component_gate_passes'] and noalloc_coverage['failed_target_windows'] == 0 and noalloc_coverage['failed_control_windows'] == 2
assert noalloc_coverage['allocation_shim_excluded'] and noalloc_coverage['numeric_precision_runtime_mutant_rejected']
bulk_phases = read('benches/results/2026-10-28-cow-bulk-phases-linux/frozen-audit.json')
assert bulk_phases['audit_complete'] and bulk_phases['processes'] == 48 and bulk_phases['wall_observations'] == 384
assert not bulk_phases['performance_qualified'] and bulk_phases['source_overlays_only_clock_hooks']
lines += ['- [Native bulk phase audit complete](../benches/results/2026-10-28-cow-bulk-phases-linux/interpretation.md): run40622638028, sourcea7d4b02c3, 48 процессов /384 wall observations /132 trace invocations. Embedding32.286-40.666%, indexes12.186-21.203%, FTS14.218-15.464%. Это профиль текущего COW, а не доказательство причины регрессии против BTreeMap. 449 release tests/2 ignored, 6 Linux smokes и независимый raw/source audit прошли; новой оптимизации не аттестует.']
mssql = read('benches/results/2026-10-28-mssql-current-cow/functional-status.json')
assert mssql['functional_raw_audit_complete'] and mssql['frozen_source_hashes_verified'] and mssql['owned_container_cleanup_verified']
assert mssql['comparative_metrics_public'] is False and mssql['full_objective_achieved'] is False
lines += [
    '- [Однооператорный CAS snapshot component gate passed](../benches/results/2026-10-28-single-stmt-cas-snapshot/report.md):source2633067cb,144процесса/1152наблюдения;Full31.932–35.768%,Normal91.371–91.663%,все4target окна6/6aboveA/A. Контроли прошли порог;native100k медианы были отрицательны в пределахA/A,поэтому нулевая регрессия не доказана.449release tests/2ignored;неверный multi-statement bypass пойман runtime-тестом. Полная матрица ещё нужна.',
    '- [Совместный COW+CAS full-source gate failed](../benches/results/2026-10-28-cow-cas-full-engine-linux/decision.md),[run40607797376](https://github.com/themoretheless/lin/actions/runs/40607797376):source36666994a,все2412процессов/19296наблюдений проверены независимым аудитом. Все10target окон прошли6/6aboveA/A: native UPDATE13.941–20.267%,scalar Full52.036–52.999%,Normal94.651–94.680%. Семь control окон провалены: Lin bulk INSERT10k в обеих сериях,filter_eq,durable UPDATE1k и три неизменённых SQLite окна. Peer wins: SQLite DELETE96/96,WAL3/96,native/read49/228,DuckDB36/36. Кандидат не принят;производственная и полная восьми-peer квалификация отсутствуют.',
    '- [RWF_DSYNC I/O probe rejected](../benches/results/2026-10-28-wal-rwf-dsync-probe-linux/report.md):run40614973547,432процесса/3456наблюдений;все16Fulltarget окон и1Normalcontrol провалены. Медианы Full от-1.435%до+3.112%,устойчивого ускорения нет.24Linuxsmoke и4error-path теста прошли,raw/source audit выполнен. Производственный код и WAL не менялись;интеграция не выполняется.',
    '- [SoA text reuse component rejected](../benches/results/2026-10-28-cow-soa-text-reuse-component-linux/decision.md):run40615737232,324процесса/2592наблюдения;5/6target окон UPDATE и2DELETEcontrol окна провалены. UPDATE100k+11.524% прошёл одну серию,+9.432% во второй не прошёл4/6. Снижение3000→0аллокаций в отдельном методе не аттестует UPDATE.453release tests/2ignored,18Linuxsmoke и независимый raw/source audit прошли;этот код исключён из текущего кандидата.',
    '- [Bulk owned-map template rejected](../benches/results/2026-10-28-cow-bulk-map-template-component-linux/decision.md):run40616712983,288процессов/2304наблюдения;все4INSERTtarget окна и2UPDATEcontrol окна провалены. Медианы INSERT от-1.500%до+0.724%;код не принят и исключён из текущего кандидата.449release tests/2ignored,16Linuxsmoke и независимый raw/source audit прошли.',
    '- [Single-Cmp DNF bypass rejected](../benches/results/2026-10-28-index-single-cmp-coverage-component-linux/decision.md):run40617087906,288процессов/2304наблюдения;filter_eq+6.279%/+6.621%,но вторая серия5/6иDuckDBcontrol-4.107%провалены.451release tests/2ignored,16Linuxsmoke и независимый raw/source audit прошли. Код исключён из текущего DELETE-кандидата;неизменённого перезапуска нет.',
    '- [DELETE WAL key projection rejected](../benches/results/2026-10-28-delete-key-wal-component-linux/decision.md):run40619276900,288процессов/2304наблюдения;Full DELETE+9.401–20.520%,но2/4target окна5/6иrollbackcontrol-2.358%провалены.453release tests/2ignored,16Linuxsmoke,8точных golden-byte проверок иcanonical-limit mutant прошли;raw/source audit завершён. Уменьшениебайтов не заменяет gate;код исключён из текущего индексного кандидата.',
    '- [Allocation-free atomic coverage rejected](../benches/results/2026-10-28-index-allocation-free-coverage-component-linux/decision.md):run40620340816,288процессов/2304наблюдения;filter_eq+10.676%/+9.739%,обе серии6/6aboveA/A. Ноpoint_get-5.130%во всех6парах одной серии иDuckDBcontrol-2.805%провалены.452release tests/2ignored,16Linuxsmoke,numericprecisionmutant иraw/source audit прошли;индекс восстановлен кa7d4b02c3.',
    '- [MSSQL private refresh](../benches/results/2026-10-28-mssql-current-cow/report.md),[run40603880905](https://github.com/themoretheless/lin/actions/runs/40603880905):замороженный предшествующий COW source75f8f6b7f;native-x86 стенд завершён,локальный raw/source audit и cleanup прошли. В репозитории только encrypted archive и функциональный статус;относительные исходы приватны. Это не публичное доказательство выигрыша и не квалификация нового CAS-кандидата.',
]
hashing = read('benches/results/2026-10-28-hashing-batch-parallel-component-linux/frozen-audit.json')
assert hashing['audit_complete'] and hashing['processes'] == 324 and hashing['observations'] == 2592
assert not hashing['component_gate_passes'] and hashing['failed_target_windows'] == 2 and hashing['failed_control_windows'] == 1
assert hashing['parallel_host_eligible'] and hashing['serial_body_preserved'] and hashing['worker_order_runtime_mutant_rejected']
lines += ['- [Parallel hashing batch rejected](../benches/results/2026-10-28-hashing-batch-parallel-component-linux/report.md): run40626189213,324процесса/2592наблюдения. Bulk INSERT10k: -33.890%/-32.935%,обе серии0/6positive;Normal UPDATE control серии2 -4.876% также провален.452release tests/2ignored,18Linuxsmoke,order mutant и независимый raw/source audit прошли. Все2064файла сохранены сSHA256;embed.rs восстановлен кa7d4b02c3. CPU/RSS измерены для всего свежего процесса,не одной операции;неизменённого перезапуска и production adoption нет.']
serial_bulk = read('benches/results/2026-10-28-bulk-serial-row-build-component-linux/frozen-audit.json')
assert serial_bulk['audit_complete'] and serial_bulk['processes'] == 324 and serial_bulk['observations'] == 2592
assert not serial_bulk['component_gate_passes'] and serial_bulk['failed_target_windows'] == 2 and serial_bulk['failed_control_windows'] == 2
assert serial_bulk['serial_row_build_only'] and serial_bulk['dropped_tail_runtime_mutant_rejected']
lines += ['- [Serial bulk row construction rejected](../benches/results/2026-10-28-bulk-serial-row-build-component-linux/report.md): run40629229553,324процесса/2592наблюдения. INSERT10k: -0.281%/-5.336%,3/6и0/6positive;INSERT1kcontrol-2.525% иfilter_eqcontrol-3.895%также провалены.449release tests/2ignored,18Linuxsmoke,tail-loss mutant и независимый raw/source audit прошли. Все2067файлов сохранены сSHA256;exec.rs восстановлен кa7d4b02c3. Уменьшение whole-process system CPU не заменяет latency gate;неизменённого перезапуска и adoption нет.']
embed_phases = read('benches/results/2026-10-28-bulk-embedding-phases-linux/frozen-audit.json')
assert embed_phases['audit_complete'] and embed_phases['processes'] == 48 and embed_phases['wall_observations'] == 384
assert not embed_phases['performance_qualified'] and embed_phases['source_overlays_match_reversible_ledger']
assert embed_phases['instrumented_release_tests'] == 449 and embed_phases['correctness_smokes'] == 6
lines += ['- [Embedding subphase audit complete](../benches/results/2026-10-28-bulk-embedding-phases-linux/interpretation.md): run40631973188,sourcea7d4b02c3,48процессов/384wall observations/132trace invocations. На10k vector allocation/copy15.304% instrumented INSERT wall,feature hashing7.343%. Traced/disabled1.1548/1.1811на10k/1k:накладные расходы существенны,точные доли обычного бинарника не установлены.449release tests/2ignored,6Linuxsmokes и независимый raw/source audit прошли;318файлов сохранены сSHA256. Это выбор следующей гипотезы,не оптимизация или peer qualification.']
sparse_output = read('benches/results/2026-10-28-hashing-sparse-output-component-linux/frozen-audit.json')
assert sparse_output['audit_complete'] and sparse_output['processes'] == 324 and sparse_output['observations'] == 2592
assert not sparse_output['component_gate_passes'] and sparse_output['failed_target_windows'] == 1 and sparse_output['failed_control_windows'] == 0
assert sparse_output['sparse_output_only'] and sparse_output['missing_coordinate_runtime_mutant_rejected'] and sparse_output['release_tests'] == 450
lines += ['- [Sparse hashing output component not accepted](../benches/results/2026-10-28-hashing-sparse-output-component-linux/report.md): run40634605768,324процесса/2592наблюдения. INSERT10k+7.790%/+8.054%;series1только5/6positive,series2 6/6,все16controlокон прошли. Проигравшая пара-0.038685%сохранена,порог6/6не ослаблялся.450release tests/2ignored,18Linuxsmokes,missing-coordinate mutant и независимый raw/source audit прошли.2067файлов сохранены сSHA256;embed.rs восстановлен кa7d4b02c3. Нет adoption/неизменённого перезапуска/полной peer qualification.']
fused_output = read('benches/results/2026-10-28-hashing-fused-output-reset-component-linux/frozen-audit.json')
assert fused_output['audit_complete'] and fused_output['processes'] == 324 and fused_output['observations'] == 2592
assert not fused_output['component_gate_passes'] and fused_output['failed_target_windows'] == 2 and fused_output['failed_control_windows'] == 1
assert fused_output['fused_output_reset_only'] and fused_output['three_runtime_mutants_rejected'] and fused_output['release_tests'] == 450
lines += ['- [Fused output/reset rejected](../benches/results/2026-10-28-hashing-fused-output-reset-component-linux/report.md): run40637176096,324процесса/2592наблюдения. INSERT10k+0.821%/+0.084%нижеA/A1.009%/2.037%,4/6и3/6positive;DuckDBfilter_eqcontrol-3.678%также провален.450release tests/2ignored,18Linuxsmokes,3runtime mutants и независимый raw/source audit прошли.2071файл сохранён сSHA256;embed.rs восстановлен кa7d4b02c3. Нет прямого сравнения с предыдущим sparse-output кандидатом;нет надёжного ускорения/adoption/неизменённого перезапуска.']
fixed_output = read('benches/results/2026-10-28-hashing-fixed-array-output-component-linux/frozen-audit.json')
assert fixed_output['audit_complete'] and fixed_output['processes'] == 324 and fixed_output['observations'] == 2592
assert not fixed_output['component_gate_passes'] and fixed_output['failed_target_windows'] == 2 and fixed_output['failed_control_windows'] == 1
assert fixed_output['fixed_array_output_only'] and fixed_output['missing_coordinate_runtime_mutant_rejected'] and fixed_output['release_tests'] == 450
lines += ['- [Safe fixed-array output rejected](../benches/results/2026-10-28-hashing-fixed-array-output-component-linux/report.md): run40640126706,324процесса/2592наблюдения. INSERT10k-1.343%/-1.566%,0/6и1/6positive;scalar Full UPDATE control серии2 -11.261%также провален.450release tests/2ignored,18Linuxsmokes,missing-coordinate mutant и независимый raw/source audit прошли.2067файлов сохранены сSHA256;embed.rs восстановлен кa7d4b02c3. Нет надёжного ускорения/adoption/неизменённого перезапуска;полная восьми-peer цель открыта.']
coverage_codegen = read('benches/results/2026-10-28-index-coverage-codegen/summary.json')
assert coverage_codegen['diagnostic_complete'] and coverage_codegen['same_direct_caller_symbols'] and not coverage_codegen['performance_qualified']
assert [r['definition_count']for r in coverage_codegen['roles']] == [1,1] and [r['direct_call_count']for r in coverage_codegen['roles']] == [18,18]
lines += ['- [Local index coverage codegen diagnosis](../benches/results/2026-10-28-index-coverage-codegen/report.md): a7d4b02c3/884ebeb81,macOS ARM64 nightly1.101 optimized library IR. Обе версии сохраняют отдельную index_covers_pred и18прямых вызовов от тех же caller symbols. Это не подтверждает гипотезу о полном встраивании проверки;не исключает partial inlining и не объясняет историческую Linux point_get регрессию. Исходники восстановлены,полный IR сохранён сSHA256;нет timing/RSS/peer qualification.']
point_contract = read('benches/results/2026-10-28-point-values-contract/verification.json')
assert point_contract['audit_complete'] and point_contract['full_value_cases'] == 6 and point_contract['runtime_mutants_rejected'] == 3 and not point_contract['performance_qualified']
lines += [
    (
        ''
    ),
    (
        '## Ограничение исторического point_get и новый контракт'
    ),
    (
        ''
    ),
    (
        '[Аудит и функциональная проверка](../benches/results/2026-10-28-point-values-contract/report.md): '
        'исторический Rust point_get материализует id/title в Lin, но SQL peers выполняют COUNT(*); read '
        'smoke этого случая не проверял значения. Старые наблюдения сохранены, однако point_get peer ratios '
        'не доказывают равную работу. Отдельный point_values возвращает одинаковые owned id/title для '
        'Lin/SQLite/DuckDB: 6 сценариев прошли full-value validator, 3 runtime подмены title отвергнуты. Это '
        'локальная функциональная проверка, без новой квалификации скорости и без покрытия остальных peers.'
    ),
]
point_local = read('benches/results/2026-10-28-point-values-local-baseline/summary.json')
assert point_local['measurement_complete'] and point_local['observations'] == 216 and point_local['processes'] == 3 and not point_local['performance_qualified']
assert len(point_local['comparisons']) == 4 and all(c['numerical_wins'] == 3 for c in point_local['comparisons'])
lines += ['', '[Локальный point_values baseline](../benches/results/2026-10-28-point-values-local-baseline/report.md): 3 свежих процесса, 216 наблюдений, одинаковые owned id/title. В обоих hit/miss Lin численно быстрее SQLite и DuckDB во всех трёх процессах. macOS ARM64, короткие samples с cap 8, без A/A: это диагностические отношения, не Linux/production qualification и не закрытие durable WAL gap. Независимый аудит контракта прошёл; семь намеренно повреждённых пакетов, включая пересчитанные SHA256, отвергнуты.']
rollback_correctness = read('benches/results/2026-10-28-rollback-hnsw-correctness/test-status.json')
assert rollback_correctness['release_exit'] == 0 and rollback_correctness['release_passed'] == 454 and rollback_correctness['release_ignored'] == 2 and not rollback_correctness['performance_qualified']
lines += [
    (
        ''
    ),
    (
        '## Новые предпосылки корректности rollback'
    ),
    (
        ''
    ),
    (
        '[Регрессии и исправление](../benches/results/2026-10-28-rollback-hnsw-correctness/report.md): '
        'восстановлена регистрация удалённых строк в HNSW и удаление узлов отменённых вставок. Смешанный '
        'DELETE→INSERT rollback терял исходные строки из-за неверного порядка; append marks теперь входят в '
        'хронологический undo journal. Локально 454 release tests прошли, 2 ignored; Linux подтверждение '
        'приведено ниже. Это новая версия экспериментального COW+CAS runtime, без переноса прежних speedup '
        'claims или production adoption. Цель восьми peers и SQLite FULL WAL gap открыты.'
    ),
]
rollback_linux = read('benches/results/2026-10-28-rollback-hnsw-correctness-linux/audit.json')
assert rollback_linux['audit_complete'] and rollback_linux['release_passed'] == 454 and rollback_linux['runtime_negatives_rejected'] == 3 and not rollback_linux['performance_qualified']
lines += ['', '[Linux rollback correctness завершён](../benches/results/2026-10-28-rollback-hnsw-correctness-linux/report.md): run40648113616, frozen21dcd7d81, Rust1.96 x86_64.454release tests/2ignored и3независимо скомпилированных runtime negatives прошли. Независимый raw/source audit и8коррупций доказательств прошли; архив полностью проверен SHA256. Это корректность, не ускорение или принятие полного исходника.']
fts_attempt = read('benches/results/2026-10-28-rollback-fts-sync-linux-attempt1/attempt-audit.json')
assert fts_attempt['failure_cause_verified'] and fts_attempt['timed_processes'] == 0 and not fts_attempt['matrix_complete']
zero_probe = read('benches/results/2026-10-28-positive-zero-msgpack-probe/legacy-status.json')
assert zero_probe['legacy_cell_cases_verified'] == 28 and zero_probe['current_sources_unchanged'] and not zero_probe['performance_qualified'] and not zero_probe['legacy_wal_replay_qualified']
lines += [
    (
        ''
    ),
    (
        '[FTS attempt1 остановлен до '
        'таймингов](../benches/results/2026-10-28-rollback-fts-sync-linux-attempt1/report.md): run40649504291'
        ' прошёл454release tests/2ignored, runtime negative и сборки; свежие smokes не начались из-за '
        'коллизии с сохранённой локальной папкой. Матрица0процессов/0наблюдений. Новое пространство выходов '
        'прошло16локальных проверок; исправленный frozen run40652603678 завершён без изменения '
        'runtime2e5146fce или критериев; отклонённый результат приведён ниже.'
    ),
    (
        ''
    ),
    (
        '[Positive-zero MessagePack '
        'probe](../benches/results/2026-10-28-positive-zero-msgpack-probe/report.md): текущий и frozen '
        'production04749994Cell-декодеры прошли28битовых случаев. Только +0.0 кодируетсяinteger0; негативная '
        'подмена -0.0 отвергнута. Конструированный sparse Cell768/704zeros:3852→1036байт. Runtime/WAL не '
        'изменены; полные legacy frames/replay, canonical quota, durability и timing не квалифицированы. Это '
        'отдельная гипотеза для Full WAL, не выигрыш benchmark.'
    ),
]
fts_completed = read('benches/results/2026-10-28-rollback-fts-sync-linux/frozen-audit.json')
assert fts_completed['audit_complete'] and fts_completed['processes'] == 288 and fts_completed['observations'] == 2304 and not fts_completed['component_gate_passes']
assert fts_completed['failed_target_windows'] == 1 and fts_completed['failed_control_windows'] == 1
lines += ['', (
    '[FTS rollback кандидат отклонён](../benches/results/2026-10-28-rollback-fts-sync-linux/report.md): '
    'run40652603678,288процессов/2304наблюдения,454release tests/2ignored,16fresh Linuxsmokes иruntime '
    'negative; независимый raw/source audit прошёл. '
    'Rollback1k+12.269%/+19.156%,5/6и6/6;10k+14.666%/+14.753%,6/6оба. Первый1k target иUPDATE1k control '
    'series2(-5.151%,A/A3.329%,2/6) провалены. Все1645raw файлов сохранены сSHA256. store.rs восстановлен'
    ' к21dcd7d81, три исправления корректности rollback сохранены. Нет принятия кандидата, неизменённого '
    'перезапуска или победы над восемью peers.'
)]
zero_full = read('benches/results/2026-11-01-wal-positive-zero/test-status.json')
zero_legacy = read('benches/results/2026-11-01-wal-positive-zero/legacy-status.json')
zero_mutants = read('benches/results/2026-11-01-wal-positive-zero/mutant-status.json')
assert zero_full['release_passed'] == 457 and zero_full['release_ignored'] == 2 and not zero_full['performance_qualified']
assert zero_legacy['legacy_cases_verified'] == 8 and zero_legacy['current_sources_unchanged'] and zero_legacy['legacy_sources_unchanged']
assert len(zero_mutants['checks']) == 2 and zero_mutants['all_sources_restored'] and all(c['compiled_runtime_rejected'] for c in zero_mutants['checks'])
lines += ['', (
    '[Full WAL positive-zero prototype](../benches/results/2026-11-01-wal-positive-zero/report.md): '
    'core62e14c7eb на исправленной базе21dcd7d81; FTS-кандидат не включён. '
    'Локально457release tests/2ignored,8реальных production04749994reader cases и2runtime negatives прошли. '
    'Normal bytes, IEEE bits, canonical payload/envelope quota и старый reader проверены;32golden файла сохранены. '
    'Это корректность прототипа, без Linux timing acceptance или production adoption. '
    'Результат нового Linux прогона приведён ниже; цель восьми peers открыта.'
)]
zero_linux = read('benches/results/2026-11-01-wal-positive-zero-component-linux/frozen-audit.json')
assert zero_linux['audit_complete'] and zero_linux['processes'] == 432 and zero_linux['observations'] == 3456
assert not zero_linux['component_gate_passes'] and zero_linux['failed_target_windows'] == 0 and zero_linux['failed_control_windows'] == 2
lines += ['', (
    '[Full WAL positive-zero Linux: rejected](../benches/results/2026-11-01-wal-positive-zero-component-linux/report.md): '
    '432 процесса/3456 наблюдений, независимый аудит пройден. Все10 целевых окон проходят: '
    'ускорение11.556–20.562%,6/6 побед в каждом. UPDATE в памяти замедлился на3.214% и2.323%; '
    'два контрольных окна провалены. Этот контроль использует Db::empty(), новый WAL-кодек не выполняется; '
    'причина регрессии не установлена. Все2385raw файлов сохранены сSHA256. '
    'Нет production adoption, отмены критерия или неизменённого перезапуска. Цель восьми peers открыта.'
)]
index_patch = read('benches/results/2026-11-01-update-index-dependencies/local-test-status.json')
index_patch_mutants = read('benches/results/2026-11-01-update-index-dependencies/mutant-status.json')
assert index_patch['release_passed'] == 455 and index_patch['release_ignored'] == 2
assert index_patch_mutants['source_restored'] and len(index_patch_mutants['checks']) == 2
assert all(c['compiled_runtime_rejected'] for c in index_patch_mutants['checks'])
lines += ['', (
    '[Selective UPDATE index maintenance: not dispatched](../benches/results/2026-11-01-update-index-dependencies/report.md): '
    '455 release tests/2 ignored и два отдельно скомпилированных runtime mutants прошли. '
    'Но текущие UPDATE targets меняют wing в единственном индексе [wing, ts]; '
    'пропускать незатронутые индексы в них нечего. Timing matrix не запускалась, '
    'ускорение не установлено. Кандидат сохранён отдельно и исключён из следующей оптимизации.'
)]
schema_probe = read('benches/results/2026-11-01-transaction-schema-undo-probe/status.json')
schema_local = read('benches/results/2026-11-01-transaction-schema-undo-fix/local-test-status.json')
assert schema_probe['runtime_reproduced'] and schema_probe['source_restored'] and len(schema_probe['outcomes']) == 6
assert all(not o['passes'] for o in schema_probe['outcomes'])
assert schema_local['exit_code'] == 0 and schema_local['release_passed'] == 458 and schema_local['release_ignored'] == 2
schema_linux = read('benches/results/2026-11-01-transaction-schema-undo-linux/frozen-audit.json')
assert schema_linux['audit_complete'] and schema_linux['release_passed'] == 458 and schema_linux['runtime_negatives_rejected'] == 3
lines += ['', (
    '[Schema rollback: Linux qualification passed](../benches/results/2026-11-01-transaction-schema-undo-linux/report.md): '
    'на точной базе21dcd7d81 воспроизведены три дефекта в6случаях: оставшаяся вставка, '
    'Store-схема и UNIQUE constraint после rollback. Core79a4cb837 сохраняет ранний undo prefix '
    'и один checkpoint для последующей части транзакции. Все458release tests/2ignored прошли; '
    'три новых теста покрывают27сценариев, включая смешанные префиксы, отказ commit и повторное использование ID. '
    '[Linux qualification](https://github.com/themoretheless/lin/actions/runs/41250934914) и независимый аудит '
    'подтвердили458tests/2ignored, три отдельно скомпилированных runtime negatives, '
    'исходники и восстановление после проверок. Измерений скорости и принятия всего engine здесь нет.'
)]
patch_attempt = read('benches/results/2026-11-01-update-patch-clone-elision-linux-attempt1/functional-audit.json')
assert patch_attempt['audit_complete'] and patch_attempt['infrastructure_failure'] and patch_attempt['timing_processes'] == patch_attempt['timing_observations'] == 0
lines += ['', (
    '[UPDATE patch clone elision: infrastructure attempt](../benches/results/2026-11-01-update-patch-clone-elision-linux-attempt1/report.md): '
    'run41249170021 прошёл455tests/2ignored и оба runtime negatives, но depth=1 checkout '
    'не загрузил исходник baseline для сборки бинарников. Измерений0; все26raw файлов и логи сохранены. '
    '[Исправленный запуск](https://github.com/themoretheless/lin/actions/runs/41251433614) использует '
    'fetch-depth:0; runtime, scenarios и thresholds совпадают с первой попыткой. '
    'Полный результат приведён ниже. Первая инфраструктурная попытка не даёт измерений скорости.'
)]
kusto_now = read('benches/results/2026-11-01-kusto-availability/status.json')
assert kusto_now['github_secret_metadata_verified'] and kusto_now['github_secret_list_complete'] and kusto_now['no_secret_values_recorded']
assert not any(kusto_now['local_environment_present'].values()) and not any(kusto_now['github_required_secret_names_present'].values())
lines += ['', (
    f"[Kusto availability](../benches/results/2026-11-01-kusto-availability/status.json), проверка {kusto_now['observed_at_utc']}: "
    'URL/DATABASE/TOKEN отсутствовали в локальной среде и полном списке repository-secret names. '
    'Значения секретов не сохранялись. Реальный Kusto-прогон остаётся открыт.'
)]
patch_linux = read('benches/results/2026-11-01-update-patch-clone-elision-linux/frozen-audit.json')
assert patch_linux['audit_complete'] and patch_linux['processes'] == 504 and patch_linux['observations'] == 4032
assert not patch_linux['component_gate_passes'] and patch_linux['failed_target_windows'] == 0 and patch_linux['failed_control_windows'] == 1
lines += ['', (
    '[UPDATE patch clone elision: overall rejected](../benches/results/2026-11-01-update-patch-clone-elision-linux/report.md): '
    '504процесса/4032наблюдения, независимый аудит пройден. Все6native UPDATE target windows '
    'дают32.295–46.902% ускорения и6/6побед, выше A/A. Все18Lin control windows проходят допуск. '
    'DuckDB filter_eq series1 провален: -11.684%, A/A8.412%,2/6. Причина не установлена; '
    'измерения сохранены, порог не отменён, неизменённого перезапуска нет. '
    'Все2695raw файлов архивированы сSHA256. Это относительное сравнение версий каждого engine, '
    'не победа Lin над DuckDB. Следующая интеграция должна включать исправление schema rollback '
    'и пройти новые полные проверки; production adoption и цель восьми peers остаются открыты.'
)]
corrected_qa = read('benches/results/2026-11-01-corrected-cow-full-engine/correctness-prerequisites.json')
assert corrected_qa['baseline']['qualification_passed'] and corrected_qa['candidate']['qualification_passed']
assert corrected_qa['baseline']['release_passed'] == 441 and corrected_qa['candidate']['release_passed'] == 458
lines += ['', (
    '[Corrected full-source prerequisites](../benches/results/2026-11-01-corrected-cow-full-engine/correctness-prerequisites.json): '
    'Linux Rust1.96 x86_64: BTreeMap e3136c0a4 — 441 tests/2ignored и6compiled negatives; '
    'COW+CAS+UPDATE clone elision625bc7180 — 458 tests/2ignored и5compiled negatives. '
    'Обе версии покрывают30schema rollback scenarios; исходные артефакты проверены независимо. '
    'Cargo dependencies иfull-value point harness совпадают побайтно. '
    'Новая полная матрица67случаев/2412процессов запущена в '
    '[CI41306613457](https://github.com/themoretheless/lin/actions/runs/41306613457); '
    'на момент фиксации запуска результата скорости ещё нет. Старые отклонённые матрицы остаются отклонёнными.'
)]
point_current = read('benches/results/2026-11-01-corrected-point-values-local/summary.json')
assert point_current['measurement_complete'] and point_current['observations'] == 216 and not point_current['performance_qualified']
lines += ['', (
    '[Current corrected COW full-value point lookup](../benches/results/2026-11-01-corrected-point-values-local/report.md): '
    'macOS ARM64,3fresh processes/216observations, runtime625bc7180. '
    'Hit Lin/SQLite0.461–0.609, Lin/DuckDB0.01295–0.01446; '
    'miss Lin/SQLite0.265–0.308, Lin/DuckDB0.00451–0.00526. '
    'Все4peer/scenario combinations дают3/3numerical wins; каждый engine возвращает полные owned id/title. '
    'Нет A/A, Linux qualification или принятия полного engine; короткие capped samples ограничивают точность. '
    'Старые локальные цифры не используются как baseline относительного ускорения.'
)]
current_peers = read('benches/results/2026-11-01-corrected-cow-peer-refresh-local/audit.json')
current_mssql = read('benches/results/2026-11-01-mssql-corrected-cow/functional-status.json')
assert current_peers['summary']['read']['comparisons'] == 108 and current_peers['summary']['native']['comparisons'] == 15
assert current_mssql['functional_raw_audit_complete'] and current_mssql['comparative_results_private']
lines += ['', (
    '[Current corrected COW available-peer refresh](../benches/results/2026-11-01-corrected-cow-peer-refresh-local/report.md): '
    'macOS ARM64 + native ARM server containers/kernel6.8.0-117; '
    '108/108read aggregate wins и324/324process comparisons; '
    '15/15native insert aggregate wins и45/45process comparisons. '
    'Минимальные aggregate ratios1.762× read и1.109× native insert, оба против DuckDB100k. '
    'Все5184samples, полные values/columns/multiplicity, sentinels иочистка проверены; '
    '76raw файлов сохранены сSHA256. Контракт prepared Rust vs Python/driver/network, '
    'не Full durable equivalence и не причинный A/B относительно старых замеров. '
    'Linux peer attempt41307182337 остановился до timings: MongoDB startup guard на kernel7.0.0-1017; '
    'точный образ воспроизведён отдельной диагностикой, обход guard не применялся.'
), '', (
    '[Current MSSQL private raw audit](../benches/results/2026-11-01-mssql-corrected-cow/functional-status.json): '
    'run41307276190, corrected runtime625bc7180,6reports/1512samples проверены независимо. '
    'В репозитории только encrypted artifact ифункциональный статус; comparative outcomes приватны. '
    'Функциональный успех не означает публично подтверждённое превосходство. '
    'Kusto иполная source acceptance остаются открыты.'
)]
corrected_full = read('benches/results/2026-11-01-corrected-cow-full-engine-linux/independent-audit.json')
assert corrected_full['audit_complete'] and corrected_full['processes']==2412 and corrected_full['observations']==19296
assert not corrected_full['source_candidate_qualified'] and len(corrected_full['failed_windows'])==8
lines += ['', (
    '[Corrected COW + CAS + UPDATE full-source matrix: rejected](../benches/results/2026-11-01-corrected-cow-full-engine-linux/decision.md): '
    'run41306613457 завершён,2412processes/19296observations пересчитаны независимо. '
    'Все10target windows проходят: native UPDATE51.106–56.538%,scalar Full63.096/63.684%, '
    'Normal97.623/97.757%, каждый6/6positive выше A/A. Все Lin controls проходят допуск. '
    'Провалены8external control windows:5SQLite и3DuckDB; причина не установлена, '
    'общий gate остаётся отклонённым без waiver илинеизменённого retry. '
    'Все15243raw файла архивированы. Текущий SQLite FULL WAL: UPDATE0/24,DELETE0/24,COMMIT0/24, '
    'ROLLBACK1/24; эти пробелы не закрыты победами в отдельном API-прогоне. '
    'Production adoption иполная цель восьми peers остаются открыты.'
)]
# Each component decision below comes from its complete frozen Linux matrix.
for slug, label, processes, observations in [
    ('rollback-index-upsert', 'Rollback index upsert', 504, 4032),
    ('fts-inline-tokens', 'FTS inline tokens', 756, 6048),
    ('fts-pending-transfer', 'FTS singleton pending transfer', 756, 6048),
    ('linux-full-fsync', 'Linux Full fsync', 720, 5760),
    ('columnar-batch-wal', 'Compatible column Batch WAL', 864, 6912),
]:
    directory = 'benches/results/2026-11-01-' + slug + '-linux/'
    qualification = read(directory + 'qualification.json')
    summary = read(directory + 'summary.json')
    windows = [r for case in summary.values() for r in case.values()]
    targets = [r for r in windows if r['target']]
    controls = [r for r in windows if not r['target']]
    assert qualification['measurement_complete'] and not qualification['component_gate_passes']
    assert qualification['processes'] == processes and qualification['observations'] == observations
    target_pass = sum(r['passes'] for r in targets)
    failed_controls = sum(not r['passes'] for r in controls)
    lines += ['', (
        f'[{label}: rejected](../{directory}report.md): '
        f'{processes} свежих процессов / {observations} наблюдений. '
        f'Прошли {target_pass}/{len(targets)} целевых окон; '
        f'провалены {failed_controls} контрольных окон. '
        'Полная матрица сохранена; принятия или неизменённого перезапуска нет. '
        'Это компонентные сравнения Lin с исходной версией, а не победы над внешними БД.'
    )]
lines += ['', '[Проверка порядка ролей](../benches/results/2026-11-01-columnar-batch-wal-linux/decision.md): в завершённой 24-case матрице роли оставались в одном порядке для каждого случая все 12 раундов. Наблюдаемые 78.780–82.589% выигрыша групповых вставок не являются аттестацией с уравновешенным порядком. Исходный gate провален, кандидат не принят; требуется новый заранее зафиксированный сбалансированный план. Другие исторические планы этим аудитом не аттестованы.']
lines += ['', '[Аудит исторического порядка](../benches/results/2026-11-02-historical-role-order-audit/report.md): по фактическим process-status.json полного COW (2412 процессов, 67 случаев), fsync (720/20) и FTS pending transfer (756/21) ни один случай не сбалансирован по позициям в обеих сериях. Постоянный порядок все 12 раундов: соответственно 56, 9 и 21 случай. Старые проваленные gate остаются проваленными; численные наблюдения не доказывают квалификацию с уравновешенным порядком. Исправленные прогоны WAL41353571509 и истории подготовки41353571356 на c73b1af46 завершены; решения приведены ниже.']
lines += ['', '[Контролируемая история подготовки: завершено](../benches/results/2026-11-02-seed-history-controlled-balanced-linux/decision.md): run41353571356, 576 свежих процессов / 4608 наблюдений, сбалансированный шестиролевой порядок, восемь Linux setup traces и независимый raw/source audit прошли. Локальный пересчёт совпал; 3128 файлов архива проверены по SHA256. Устойчивого выигрыша grouped seed нет: 0/16 окон с 6/6 положительными парами выше A/A; один same-binary null control вне диапазона. Grouped Lin/SQLite по медиане12 пар: UPDATE1k1.077,10k1.576; DELETE1k1.089,10k1.612; COMMIT1k1.083,10k1.583; ROLLBACK1k1.685,10k2.774. Grouped rollback10k медленнее в обеих сериях. Разрыв не закрыт, физическая причина не изолирована, исходная fixture сохраняется. Исправленные COW41353987944 и WAL41353571509 завершены с отклонением кандидатов; решения приведены ниже.']
lines += ['', '[Balanced column Batch WAL: rejected](../benches/results/2026-11-02-columnar-batch-wal-balanced-linux/decision.md): run41353571509, 864 processes/6912 observations. Balanced raw order and independent audit passed; 4710 files rehashed. All four insertion targets pass (+79.070 to +82.467%, 6/6), but UPDATE10k controls fail: durable series1 -3.882% vs A/A3.564%; warm series2 -5.234% vs A/A2.665%. Cause unproven; warm control does not execute WAL. SQLite FULL WAL: 0/96 wins. No adoption or unchanged retry. Corrected full COW run41353987944 completed and remains rejected; see below.']
lines += ['', '[Balanced complete COW comparison: rejected](../benches/results/2026-11-02-corrected-cow-balanced-linux/decision.md): run41353987944, 2412 processes/19296 observations, 134 smokes, balanced per-case order and independent raw audit passed. Local reconstruction matches CI; all 15243 archived files rehashed. All ten target windows pass: native UPDATE +50.899 to +56.828%, scalar FULL +63.167/+63.342%, NORMAL +97.529/+97.673%. Four controls fail: Lin durable COMMIT1k series2 -6.793% vs A/A6.343%; SQLite DELETE10k series1 -10.839% vs A/A4.965%; DuckDB GROUP BY both series -14.419/-14.991% vs A/A8.902/3.870%. Causes unproven; no waiver. Candidate peer wins245/456; SQLite FULL WAL0/96. No adoption or unchanged retry; eight-peer objective remains open.']
lines += ['', '[Crossed identical-binary controls](https://github.com/themoretheless/lin/blob/f06b40f474e003002140232b2420814492ed3b30/benches/results/2026-11-02-crossed-control-diagnostic-linux/decision.md): run41363558777 succeeded, 216 processes/1728 observations; all2075 raw files rehashed and both exact Linux ELF hashes verified. Local reconstruction matches CI. Three labels per identical executable. DuckDB GROUP BY candidate latency increases33.244%/34.675%, with0/6 positive gain rounds in each series. Null-label contrasts retained, including candidate ab+11.631% in series2. SQLite DELETE contrasts -0.040%/-1.409%; Lin COMMIT+2.797%/+4.286%,5/6 each. Original failures remain binding; no causal mechanism, waiver or adoption established. Exact executables are retained in the separate CI artifact for inspection. FTS field-impact run41366596325 has now completed and remains rejected; see the audited result below.']
lines += ['', '[FTS rollback field-impact: rejected](https://github.com/themoretheless/lin/blob/79b247bcabe21ebdb1ca2d88427c1e41b06a955d/benches/results/2026-11-02-rollback-fts-field-impact-linux/decision.md): run41366596325,720 processes/5760 observations,460 passed/2 ignored,two compiled negatives,40 smokes, balanced order and independent audit passed. Local reconstruction matches; all3849 raw files and both exact Linux executable hashes verified. All10 target windows pass6/6 above A/A: warm rollback1k+27.311/+26.743%,10k+23.499/+23.771%,100k+24.223/+23.464%; durable rollback1k+26.207/+24.320%,10k+19.862/+20.110%. Eight controls fail, including DuckDB GROUP BY -14.757/-7.031%, scalar FULL series2 -10.830%, durable DELETE1k series2 -4.884%. In this run only, matched rollback Lin/SQLite WAL ratio1k0.699008 (12/12 numeric wins),10k1.370044 (0/12); not a complete durability matrix. No adoption, noise waiver or unchanged retry.']
lines += ['', '[DuckDB instruction-address audit](https://github.com/themoretheless/lin/blob/78a63ee1e4c9434edb6d52086a4e38ceb2649bfa/benches/results/2026-11-02-duckdb-address-operand-audit/README.md): exact crossed-control ELF hashes checked. Among43290 unambiguous common DuckDB functions,5914 raw bodies match; all37376 remaining bodies match after masking branch immediates and validated RIP-relative disp32, with identical instruction IDs/lengths and complete decoding. Initial SSE decoder width defect and its regression are retained. No target-identity equivalence or latency cause proved. Source/runtime acceptance remains open.']
lines += ['', '[Package codegen-units=1: local discovery](https://github.com/themoretheless/lin/blob/77d61762d19d1ae85d51afcfa4d4111c0518648f/benches/results/2026-11-02-cow-package-codegen1-local/report.md): fixed625 runtime,216 macOS processes/1728 observations,24 smokes, balanced six-round schedule and independent raw/hash/command/numerical audit passed. All1219 archived files rehashed. UPDATE10k +22.443%,6/6, A/A4.325%; other warm targets mixed or below A/A. FULL COMMIT1k -6.456%,1/6, A/A5.199%; several controls have large local A/A. No production profile change, fresh full correctness qualification or Linux promotion; no FTS changes combined.']
output = root / 'docs/benchmark-goal-status-2026-10-27.md'
output.write_text('\n'.join(lines) + '\n')
# Never expose endpoint values or credentials in the status artifact.
print(f'Wrote {output.relative_to(root)}; historical 384 mutation comparisons and 108 read comparisons; objective remains open')
