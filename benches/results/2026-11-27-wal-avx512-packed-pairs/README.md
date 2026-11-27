# Упаковка разрежённых пар WAL инструкциями AVX-512

Кандидат 5c782c08e, базовый движок fe2717127. По флагам CPU AVX512F доступен только на wal-full-amd-2 (EPYC 9V74). Три EPYC 7763 и ARM64 используют прежний путь. Клиент и SQLite встроены локально на одном раннере, TCMalloc на Linux x64 и штатный аллокатор на ARM64. Тесты модуля persist предшествуют бенчу. На размер и раннер выполнено по 24 пары с проверкой восстановления. Разницы указаны в миллисекундах.

| Движок | CPU | Раннер | Строк | INSERT | Δ к main, мс | Кандидат − движок, мс | main − движок, мс | Побед над движком |
|---|---|---|---:|---|---:|---:|---:|---:|
| sqlite | AMD EPYC 7763 64-Core Processor | wal-full-amd-1 | 1000 | WAL/FULL | +0.019 | +0.274 | +0.205 | 4/24 |
| sqlite | AMD EPYC 7763 64-Core Processor | wal-full-amd-1 | 10000 | WAL/FULL | +0.230 | -7.543 | -7.783 | 24/24 |
| sqlite | AMD EPYC 9V74 80-Core Processor | wal-full-amd-2 | 1000 | WAL/FULL | +0.044 | +0.514 | +0.454 | 0/24 |
| sqlite | AMD EPYC 9V74 80-Core Processor | wal-full-amd-2 | 10000 | WAL/FULL | +1.433 | +3.701 | +2.302 | 2/24 |
| sqlite | AMD EPYC 7763 64-Core Processor | wal-full-amd-3 | 1000 | WAL/FULL | +0.006 | +0.219 | +0.192 | 0/24 |
| sqlite | AMD EPYC 7763 64-Core Processor | wal-full-amd-3 | 10000 | WAL/FULL | -0.239 | -7.467 | -7.494 | 24/24 |
| sqlite | AMD EPYC 7763 64-Core Processor | wal-full-amd-4 | 1000 | WAL/FULL | -0.040 | +0.242 | +0.277 | 1/24 |
| sqlite | AMD EPYC 7763 64-Core Processor | wal-full-amd-4 | 10000 | WAL/FULL | -0.135 | -7.220 | -7.100 | 24/24 |
| sqlite | Neoverse-N2 | wal-full-arm64 | 1000 | WAL/FULL | +0.050 | +0.440 | +0.413 | 1/24 |
| sqlite | Neoverse-N2 | wal-full-arm64 | 10000 | WAL/FULL | -0.416 | -7.896 | -7.435 | 24/24 |

На единственном поддерживаемом раннере кандидат проиграл main на обоих размерах.

[Исходные измерения и результаты тестов](https://github.com/themoretheless/lin/actions/runs/46027785763)
