# Резервирование буфера по первой строке раздела WAL

Кандидат 65659a2e0, контроль fe2717127. Изменение применяется на Linux x64 при первом заполнении пустого буфера. Локальные встроенные движки, TCMalloc на x64 и штатный аллокатор на ARM64. Медианы парных разниц в мс. По 24 пары с проверкой восстановленных строк на размер и раннер.

| Движок | CPU | Раннер | Строк | INSERT | Δ к main, мс | Кандидат − движок, мс | main − движок, мс | Побед над движком |
|---|---|---|---:|---|---:|---:|---:|---:|
| sqlite | AMD EPYC 9V74 80-Core Processor | wal-full-amd-1 | 1000 | WAL/FULL | -0.053 | +0.850 | +0.903 | 1/24 |
| sqlite | AMD EPYC 9V74 80-Core Processor | wal-full-amd-1 | 10000 | WAL/FULL | +0.000 | +3.008 | +3.286 | 4/24 |
| sqlite | AMD EPYC 9V74 80-Core Processor | wal-full-amd-2 | 1000 | WAL/FULL | -0.076 | +0.338 | +0.445 | 1/24 |
| sqlite | AMD EPYC 9V74 80-Core Processor | wal-full-amd-2 | 10000 | WAL/FULL | +0.163 | +2.296 | +2.281 | 3/24 |
| sqlite | AMD EPYC 9V74 80-Core Processor | wal-full-amd-4 | 1000 | WAL/FULL | -0.080 | +0.328 | +0.380 | 0/24 |
| sqlite | AMD EPYC 9V74 80-Core Processor | wal-full-amd-4 | 10000 | WAL/FULL | -0.250 | +2.855 | +2.850 | 4/24 |
| sqlite | Neoverse-N2 | wal-full-arm64 | 1000 | WAL/FULL | +0.119 | +0.406 | +0.341 | 0/24 |
| sqlite | Neoverse-N2 | wal-full-arm64 | 10000 | WAL/FULL | +0.327 | -2.346 | -2.122 | 22/24 |

На трёх EPYC 9V74 кандидат ускорил 1k относительно контроля на 0,053–0,080 мс, но проиграл SQLite в 70 из 72 пар. На 10k направление эффекта зависит от раннера. ARM64 служит контролем воспроизводимости, поскольку изменённый код на нём не компилируется.

[Исходные измерения](https://github.com/themoretheless/lin/actions/runs/46028517058)
