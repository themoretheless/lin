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
peer_path = 'benches/results/2026-10-26-persistent-peer-refresh/audit.json'
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
    f"[API read refresh](../benches/results/2026-10-26-persistent-peer-refresh/report.md) относится к `{peer['source_commit']}`: 1k/10k/100k, три процесса, подготовленные запросы и полный readback. Минимумы ниже — среди сохранённых сравнений чтения этого прогона.", '',
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
    '[MSSQL qualification](../benches/results/2026-10-26-mssql-native-ci/report.md) относится к frozen `8b0e9717`, не к текущим исходникам; comparative evidence остаётся приватным. Сравнение API включает Python bindings у embedded peers и driver/network у серверов, а не только CPU движков. Native insertion в API refresh использует разные объявленные durability-контракты и tmpfs у серверов: 15/15 агрегированных побед, 44/45 процессных; один проигрыш DuckDB на 100k сохранён. Это не замена Full durable gate.', '',
    '## Текущие независимые проверки', '',
    '- [WAL generation/atomic frame repair](../benches/results/2026-10-27-tx-wal-generation/report.md): rollback, один frame на commit, checkpoint error и выход дочернего процесса без Drop. Локально 8 регрессий; [Linux run 40529581051](https://github.com/themoretheless/lin/actions/runs/40529581051).',
    '- [Abort после ошибки транзакции](../benches/results/2026-10-27-tx-quota-abort/report.md): quota/CAS/parse error не позволяют commit; локально полный набор 420 passed /2 ignored; [Linux run 40531748030](https://github.com/themoretheless/lin/actions/runs/40531748030). Это более новый exec, он не включён в frozen WAL timing input.',
    '- [Bounded Linux KEEP_SIZE](../benches/results/2026-10-27-linux-wal-keep-size-tx-fixed/report.md): исправленный независимый CRC32C-эталон и прежние replay/follower/checkpoint checks; локально 417 passed /2 ignored; [Linux run 40530054164](https://github.com/themoretheless/lin/actions/runs/40530054164).',
    '- [Full-engine WAL reserve protocol](../benches/results/2026-10-27-linux-wal-reserve-engine/protocol.md): 61 case, 2196 processes, 17568 observations; baseline/control/candidate. [Run 40530861048](https://github.com/themoretheless/lin/actions/runs/40530861048) сначала проверяет Linux-корректность, затем измеряет. Подготовка и 122 локальных smoke-проверки не являются измеренным ускорением.',
    '- [Незавершённый lazy COW](../benches/results/2026-10-27-cow-lazy-sharing-engine-linux-incomplete/report.md): отменённый run; 1207 status-файлов из 1332 требуемых процессов, итоговый gate не выполнялся. Принятого ускорения нет; публичный Row нового типа не включён в production.', '',
    'Состояние выполнения CI проверяется по самим run: этот документ фиксирует объём и ссылки, а не предсказывает окончательный статус. Для принятия WAL-кандидата нужны все целевые пары и все контроли; затем требуется повтор затронутых контрактов на итоговом исправленном бинарнике. Полная цель по всем восьми peers **не достигнута**.', '',
    '## Воспроизведение сводки', '',
    '`python3 scripts/write-benchmark-goal-status.py` читает сохранённые JSON и проверяет количества/источники. Он не запускает СУБД и не заменяет независимый raw audit исходных экспериментов.']
output = root / 'docs/benchmark-goal-status-2026-10-27.md'
output.write_text('\n'.join(lines) + '\n')
# Never expose endpoint values or credentials in the status artifact.
print(f'Wrote {output.relative_to(root)}; historical 384 mutation comparisons and 108 read comparisons; objective remains open')
