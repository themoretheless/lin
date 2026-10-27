# Full-sync mutation baseline vs SQLite

Source qualification of an optimization is not claimed. Clean-reopen readback is not crash proof. Both SQLite FULL DELETE and WAL results retained. Ratio <1 means Lin faster. Each row contains six within-round peer comparisons; baseline/control are independent runs of the same executable. All process losses retained.

| Case | Peer | Role | Series | Lin ns | SQLite ns | Lin/SQLite | Lin wins | Lin A/A % | SQLite A/A % |
|---|---|---|---|---:|---:|---:|---:|---:|---:|
| compare/durable_update_1row_1k/lin | sqlite_delete | baseline | series1 | 383524.50 | 772886.00 | 0.4960 | 6/6 | 2.182 | 3.923 |
| compare/durable_update_1row_1k/lin | sqlite_delete | baseline | series2 | 406476.00 | 884197.00 | 0.4600 | 6/6 | 2.246 | 3.409 |
| compare/durable_update_1row_1k/lin | sqlite_delete | control | series1 | 383236.75 | 789129.25 | 0.4816 | 6/6 | 2.182 | 3.923 |
| compare/durable_update_1row_1k/lin | sqlite_delete | control | series2 | 382155.00 | 817274.50 | 0.4670 | 6/6 | 2.246 | 3.409 |
| compare/durable_update_1row_1k/lin | sqlite_wal | baseline | series1 | 383524.50 | 304397.25 | 1.2561 | 0/6 | 2.182 | 9.451 |
| compare/durable_update_1row_1k/lin | sqlite_wal | baseline | series2 | 406476.00 | 296435.00 | 1.3445 | 0/6 | 2.246 | 4.817 |
| compare/durable_update_1row_1k/lin | sqlite_wal | control | series1 | 383236.75 | 311693.25 | 1.2333 | 0/6 | 2.182 | 9.451 |
| compare/durable_update_1row_1k/lin | sqlite_wal | control | series2 | 382155.00 | 291279.75 | 1.3014 | 0/6 | 2.246 | 4.817 |
| compare/durable_delete_1row_1k/lin | sqlite_delete | baseline | series1 | 394691.25 | 869279.00 | 0.4476 | 6/6 | 4.873 | 9.296 |
| compare/durable_delete_1row_1k/lin | sqlite_delete | baseline | series2 | 384414.50 | 868463.00 | 0.4515 | 6/6 | 7.857 | 10.914 |
| compare/durable_delete_1row_1k/lin | sqlite_delete | control | series1 | 386195.25 | 902865.00 | 0.4385 | 6/6 | 4.873 | 9.296 |
| compare/durable_delete_1row_1k/lin | sqlite_delete | control | series2 | 373429.00 | 970491.25 | 0.4043 | 6/6 | 7.857 | 10.914 |
| compare/durable_delete_1row_1k/lin | sqlite_wal | baseline | series1 | 394691.25 | 295132.50 | 1.2486 | 0/6 | 4.873 | 8.041 |
| compare/durable_delete_1row_1k/lin | sqlite_wal | baseline | series2 | 384414.50 | 317464.25 | 1.2283 | 0/6 | 7.857 | 6.757 |
| compare/durable_delete_1row_1k/lin | sqlite_wal | control | series1 | 386195.25 | 303047.00 | 1.2656 | 0/6 | 4.873 | 8.041 |
| compare/durable_delete_1row_1k/lin | sqlite_wal | control | series2 | 373429.00 | 313243.75 | 1.1873 | 0/6 | 7.857 | 6.757 |
| compare/durable_commit_1row_1k/lin | sqlite_delete | baseline | series1 | 363545.50 | 835448.25 | 0.4494 | 6/6 | 3.321 | 5.772 |
| compare/durable_commit_1row_1k/lin | sqlite_delete | baseline | series2 | 396571.75 | 864863.25 | 0.4546 | 6/6 | 1.866 | 4.185 |
| compare/durable_commit_1row_1k/lin | sqlite_delete | control | series1 | 377208.75 | 838509.25 | 0.4233 | 6/6 | 3.321 | 5.772 |
| compare/durable_commit_1row_1k/lin | sqlite_delete | control | series2 | 379622.75 | 865500.00 | 0.4451 | 6/6 | 1.866 | 4.185 |
| compare/durable_commit_1row_1k/lin | sqlite_wal | baseline | series1 | 363545.50 | 304007.00 | 1.1704 | 0/6 | 3.321 | 3.990 |
| compare/durable_commit_1row_1k/lin | sqlite_wal | baseline | series2 | 396571.75 | 295092.00 | 1.3217 | 0/6 | 1.866 | 2.415 |
| compare/durable_commit_1row_1k/lin | sqlite_wal | control | series1 | 377208.75 | 288256.50 | 1.3089 | 0/6 | 3.321 | 3.990 |
| compare/durable_commit_1row_1k/lin | sqlite_wal | control | series2 | 379622.75 | 303007.00 | 1.2418 | 0/6 | 1.866 | 2.415 |
| compare/durable_rollback_1row_1k/lin | sqlite_delete | baseline | series1 | 31095.75 | 119666.75 | 0.2577 | 6/6 | 5.930 | 1.421 |
| compare/durable_rollback_1row_1k/lin | sqlite_delete | baseline | series2 | 31216.25 | 122842.25 | 0.2514 | 6/6 | 6.355 | 1.925 |
| compare/durable_rollback_1row_1k/lin | sqlite_delete | control | series1 | 29164.25 | 120260.00 | 0.2446 | 6/6 | 5.930 | 1.421 |
| compare/durable_rollback_1row_1k/lin | sqlite_delete | control | series2 | 28102.50 | 120162.00 | 0.2349 | 6/6 | 6.355 | 1.925 |
| compare/durable_rollback_1row_1k/lin | sqlite_wal | baseline | series1 | 31095.75 | 20137.50 | 1.5393 | 0/6 | 5.930 | 0.612 |
| compare/durable_rollback_1row_1k/lin | sqlite_wal | baseline | series2 | 31216.25 | 20190.25 | 1.5269 | 0/6 | 6.355 | 1.422 |
| compare/durable_rollback_1row_1k/lin | sqlite_wal | control | series1 | 29164.25 | 20142.50 | 1.4590 | 0/6 | 5.930 | 0.612 |
| compare/durable_rollback_1row_1k/lin | sqlite_wal | control | series2 | 28102.50 | 20265.50 | 1.3804 | 0/6 | 6.355 | 1.422 |
| compare/durable_update_1row_10k/lin | sqlite_delete | baseline | series1 | 587905.25 | 910158.25 | 0.6427 | 6/6 | 2.970 | 5.412 |
| compare/durable_update_1row_10k/lin | sqlite_delete | baseline | series2 | 557313.00 | 911866.50 | 0.6078 | 6/6 | 3.638 | 8.548 |
| compare/durable_update_1row_10k/lin | sqlite_delete | control | series1 | 597731.00 | 924798.25 | 0.6643 | 6/6 | 2.970 | 5.412 |
| compare/durable_update_1row_10k/lin | sqlite_delete | control | series2 | 547687.00 | 888684.75 | 0.6149 | 6/6 | 3.638 | 8.548 |
| compare/durable_update_1row_10k/lin | sqlite_wal | baseline | series1 | 587905.25 | 319492.75 | 1.8258 | 0/6 | 2.970 | 8.834 |
| compare/durable_update_1row_10k/lin | sqlite_wal | baseline | series2 | 557313.00 | 319923.75 | 1.7460 | 0/6 | 3.638 | 4.876 |
| compare/durable_update_1row_10k/lin | sqlite_wal | control | series1 | 597731.00 | 337747.00 | 1.7412 | 0/6 | 2.970 | 8.834 |
| compare/durable_update_1row_10k/lin | sqlite_wal | control | series2 | 547687.00 | 332795.00 | 1.7107 | 0/6 | 3.638 | 4.876 |
| compare/durable_delete_1row_10k/lin | sqlite_delete | baseline | series1 | 598272.25 | 941040.75 | 0.6267 | 6/6 | 3.258 | 4.002 |
| compare/durable_delete_1row_10k/lin | sqlite_delete | baseline | series2 | 535531.75 | 945088.50 | 0.5536 | 6/6 | 6.892 | 2.176 |
| compare/durable_delete_1row_10k/lin | sqlite_delete | control | series1 | 590212.25 | 940332.00 | 0.6131 | 6/6 | 3.258 | 4.002 |
| compare/durable_delete_1row_10k/lin | sqlite_delete | control | series2 | 564160.75 | 927917.00 | 0.5912 | 6/6 | 6.892 | 2.176 |
| compare/durable_delete_1row_10k/lin | sqlite_wal | baseline | series1 | 598272.25 | 332384.25 | 1.7953 | 0/6 | 3.258 | 2.010 |
| compare/durable_delete_1row_10k/lin | sqlite_wal | baseline | series2 | 535531.75 | 316642.75 | 1.6273 | 0/6 | 6.892 | 2.073 |
| compare/durable_delete_1row_10k/lin | sqlite_wal | control | series1 | 590212.25 | 335502.25 | 1.7844 | 0/6 | 3.258 | 2.010 |
| compare/durable_delete_1row_10k/lin | sqlite_wal | control | series2 | 564160.75 | 326350.75 | 1.7548 | 0/6 | 6.892 | 2.073 |
| compare/durable_commit_1row_10k/lin | sqlite_delete | baseline | series1 | 541282.75 | 893952.75 | 0.6116 | 6/6 | 1.517 | 6.233 |
| compare/durable_commit_1row_10k/lin | sqlite_delete | baseline | series2 | 540599.50 | 889139.25 | 0.6301 | 6/6 | 2.916 | 18.797 |
| compare/durable_commit_1row_10k/lin | sqlite_delete | control | series1 | 549508.25 | 920725.75 | 0.6065 | 6/6 | 1.517 | 6.233 |
| compare/durable_commit_1row_10k/lin | sqlite_delete | control | series2 | 538821.00 | 940107.00 | 0.5781 | 6/6 | 2.916 | 18.797 |
| compare/durable_commit_1row_10k/lin | sqlite_wal | baseline | series1 | 541282.75 | 336770.25 | 1.6405 | 0/6 | 1.517 | 3.402 |
| compare/durable_commit_1row_10k/lin | sqlite_wal | baseline | series2 | 540599.50 | 332414.50 | 1.6592 | 0/6 | 2.916 | 4.699 |
| compare/durable_commit_1row_10k/lin | sqlite_wal | control | series1 | 549508.25 | 335948.75 | 1.6444 | 0/6 | 1.517 | 3.402 |
| compare/durable_commit_1row_10k/lin | sqlite_wal | control | series2 | 538821.00 | 325013.25 | 1.6965 | 0/6 | 2.916 | 4.699 |
| compare/durable_rollback_1row_10k/lin | sqlite_delete | baseline | series1 | 41931.00 | 140956.50 | 0.2948 | 6/6 | 2.302 | 3.284 |
| compare/durable_rollback_1row_10k/lin | sqlite_delete | baseline | series2 | 41568.00 | 139831.50 | 0.3016 | 6/6 | 1.088 | 1.986 |
| compare/durable_rollback_1row_10k/lin | sqlite_delete | control | series1 | 41921.25 | 145577.25 | 0.2859 | 6/6 | 2.302 | 3.284 |
| compare/durable_rollback_1row_10k/lin | sqlite_delete | control | series2 | 42119.00 | 140052.25 | 0.3008 | 6/6 | 1.088 | 1.986 |
| compare/durable_rollback_1row_10k/lin | sqlite_wal | baseline | series1 | 41931.00 | 28956.75 | 1.4590 | 0/6 | 2.302 | 7.005 |
| compare/durable_rollback_1row_10k/lin | sqlite_wal | baseline | series2 | 41568.00 | 28078.00 | 1.5037 | 0/6 | 1.088 | 3.722 |
| compare/durable_rollback_1row_10k/lin | sqlite_wal | control | series1 | 41921.25 | 30094.25 | 1.4014 | 0/6 | 2.302 | 7.005 |
| compare/durable_rollback_1row_10k/lin | sqlite_wal | control | series2 | 42119.00 | 25553.00 | 1.6458 | 0/6 | 1.088 | 3.722 |

Numeric Lin wins: 192/384. Full eight-peer objective remains unachieved.

## Whole-process peak RSS (KiB)

This includes setup, oracle and reopen; it is not isolated mutation memory.

| Case | Series | Baseline | Control |
|---|---|---:|---:|
| compare/durable_update_1row_1k/lin | series1 | 71870.0 | 71886.0 |
| compare/durable_update_1row_1k/lin | series2 | 71908.0 | 71788.0 |
| compare/durable_update_1row_1k/sqlite_delete | series1 | 13446.0 | 13350.0 |
| compare/durable_update_1row_1k/sqlite_delete | series2 | 13510.0 | 13366.0 |
| compare/durable_update_1row_1k/sqlite_wal | series1 | 13398.0 | 13452.0 |
| compare/durable_update_1row_1k/sqlite_wal | series2 | 13308.0 | 13492.0 |
| compare/durable_delete_1row_1k/lin | series1 | 71786.0 | 71738.0 |
| compare/durable_delete_1row_1k/lin | series2 | 71806.0 | 71752.0 |
| compare/durable_delete_1row_1k/sqlite_delete | series1 | 13610.0 | 13542.0 |
| compare/durable_delete_1row_1k/sqlite_delete | series2 | 13540.0 | 13584.0 |
| compare/durable_delete_1row_1k/sqlite_wal | series1 | 13426.0 | 13464.0 |
| compare/durable_delete_1row_1k/sqlite_wal | series2 | 13534.0 | 13444.0 |
| compare/durable_commit_1row_1k/lin | series1 | 71880.0 | 71870.0 |
| compare/durable_commit_1row_1k/lin | series2 | 71864.0 | 71822.0 |
| compare/durable_commit_1row_1k/sqlite_delete | series1 | 13436.0 | 13378.0 |
| compare/durable_commit_1row_1k/sqlite_delete | series2 | 13548.0 | 13388.0 |
| compare/durable_commit_1row_1k/sqlite_wal | series1 | 13484.0 | 13388.0 |
| compare/durable_commit_1row_1k/sqlite_wal | series2 | 13492.0 | 13396.0 |
| compare/durable_rollback_1row_1k/lin | series1 | 71922.0 | 71858.0 |
| compare/durable_rollback_1row_1k/lin | series2 | 71948.0 | 71906.0 |
| compare/durable_rollback_1row_1k/sqlite_delete | series1 | 13438.0 | 13438.0 |
| compare/durable_rollback_1row_1k/sqlite_delete | series2 | 13298.0 | 13226.0 |
| compare/durable_rollback_1row_1k/sqlite_wal | series1 | 13532.0 | 13544.0 |
| compare/durable_rollback_1row_1k/sqlite_wal | series2 | 13486.0 | 13564.0 |
| compare/durable_update_1row_10k/lin | series1 | 592580.0 | 592750.0 |
| compare/durable_update_1row_10k/lin | series2 | 592798.0 | 592736.0 |
| compare/durable_update_1row_10k/sqlite_delete | series1 | 19826.0 | 19930.0 |
| compare/durable_update_1row_10k/sqlite_delete | series2 | 19828.0 | 19834.0 |
| compare/durable_update_1row_10k/sqlite_wal | series1 | 19952.0 | 19832.0 |
| compare/durable_update_1row_10k/sqlite_wal | series2 | 19800.0 | 19888.0 |
| compare/durable_delete_1row_10k/lin | series1 | 592410.0 | 592456.0 |
| compare/durable_delete_1row_10k/lin | series2 | 592592.0 | 592496.0 |
| compare/durable_delete_1row_10k/sqlite_delete | series1 | 19946.0 | 19828.0 |
| compare/durable_delete_1row_10k/sqlite_delete | series2 | 19852.0 | 19830.0 |
| compare/durable_delete_1row_10k/sqlite_wal | series1 | 19884.0 | 19796.0 |
| compare/durable_delete_1row_10k/sqlite_wal | series2 | 19806.0 | 19894.0 |
| compare/durable_commit_1row_10k/lin | series1 | 592716.0 | 592786.0 |
| compare/durable_commit_1row_10k/lin | series2 | 592758.0 | 592824.0 |
| compare/durable_commit_1row_10k/sqlite_delete | series1 | 19922.0 | 19890.0 |
| compare/durable_commit_1row_10k/sqlite_delete | series2 | 19940.0 | 19904.0 |
| compare/durable_commit_1row_10k/sqlite_wal | series1 | 19910.0 | 19714.0 |
| compare/durable_commit_1row_10k/sqlite_wal | series2 | 19874.0 | 19852.0 |
| compare/durable_rollback_1row_10k/lin | series1 | 592682.0 | 592642.0 |
| compare/durable_rollback_1row_10k/lin | series2 | 592698.0 | 592724.0 |
| compare/durable_rollback_1row_10k/sqlite_delete | series1 | 19932.0 | 20026.0 |
| compare/durable_rollback_1row_10k/sqlite_delete | series2 | 19980.0 | 19902.0 |
| compare/durable_rollback_1row_10k/sqlite_wal | series1 | 19850.0 | 19726.0 |
| compare/durable_rollback_1row_10k/sqlite_wal | series2 | 19884.0 | 19964.0 |
