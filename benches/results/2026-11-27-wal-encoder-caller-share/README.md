# Увеличенная доля работы вызывающего потока при записи WAL

Кандидат 7bac7c387, контроль fe2717127 с той же измерительной утилитой. Четыре Linux-раннера с целевыми CPU выполнили по 24 пары на размер с полной проверкой восстановленных строк. Клиент и SQLite работали на том же хосте. На x64 Lin использовал статический TCMalloc, на ARM64 штатный аллокатор. Медианы парных разниц указаны в мс.

| Движок | CPU | Раннер | Строк | INSERT | Δ к main, мс | Кандидат − движок, мс | main − движок, мс | Побед над движком |
|---|---|---|---:|---|---:|---:|---:|---:|
| sqlite | AMD EPYC 9V74 80-Core Processor | wal-full-amd-2 | 1000 | WAL/FULL | -0.053 | +0.346 | +0.395 | 0/24 |
| sqlite | AMD EPYC 9V74 80-Core Processor | wal-full-amd-2 | 10000 | WAL/FULL | +0.178 | +2.042 | +2.089 | 3/24 |
| sqlite | AMD EPYC 9V45 96-Core Processor | wal-full-amd-3 | 1000 | WAL/FULL | -0.039 | -0.209 | -0.166 | 23/24 |
| sqlite | AMD EPYC 9V45 96-Core Processor | wal-full-amd-3 | 10000 | WAL/FULL | -0.344 | -0.730 | +0.106 | 15/24 |
| sqlite | AMD EPYC 9V74 80-Core Processor | wal-full-amd-4 | 1000 | WAL/FULL | -0.022 | +0.358 | +0.439 | 1/24 |
| sqlite | AMD EPYC 9V74 80-Core Processor | wal-full-amd-4 | 10000 | WAL/FULL | -0.238 | +2.368 | +4.020 | 10/24 |
| sqlite | Neoverse-N2 | wal-full-arm64 | 1000 | WAL/FULL | +0.037 | +0.337 | +0.297 | 0/24 |
| sqlite | Neoverse-N2 | wal-full-arm64 | 10000 | WAL/FULL | -0.139 | -8.043 | -7.866 | 23/24 |

Локальное сравнение полного WAL до закрытия базы на восьми размерах показало побайтовое совпадение. Размеры и SHA-256 журналов:

```text
511 171409 8acc35b8dd6651fae25db4bda772a6c39b972c9b0994c8464e1fda28603b5aca
512 171747 f6a0fe0fddb38e820fe9a65bc6f169b0749af4bb15d7f55c106d2e1de06806b9
513 172085 6f73321376c5235a018613817accca5d93c815e04cc053d9bc4361a52409a1ae
999 336753 67fbbba2347506dd797ad5479120515ba2beae2d669ac76eca3e2b4ff49bc020
1000 337083 7112ede9deab5a043193c163afc20f9a38561deb932860044f84237c6df94c15
1001 337452 da2c96c685a52e97440025d7e3558428b24e878aefe30e5c903576795e164de3
4097 1445571 353f5d5a0b75374050c06101b22c0b0f2df1f5c3238b918a28b099fb79981cdb
10000 3558455 f419965cdb310ee26772a08f9ad85d1ce881ac4674a9d8a4128eb1f485908579
```

[Парный прогон](https://github.com/themoretheless/lin/actions/runs/46024761209)
