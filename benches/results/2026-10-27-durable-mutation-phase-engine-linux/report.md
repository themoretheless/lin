# Durable mutation caller-thread phase diagnostic

Original/control use the same uninstrumented production-source binary. Disabled/traced use the same instrumented binary. A/A is median absolute within-round difference. Ratios >1 mean slower. This is diagnostic evidence, not acceptance of an optimization.

## Measured benchmark wall observations

| Case | Original ns | Original A/A % | Disabled/original | Traced/original | Traced/disabled |
|---|---:|---:|---:|---:|---:|
| compare/durable_update_1row_1k/lin | 339385.50 | 6.593 | 0.9387 | 0.9590 | 1.0087 |
| compare/durable_update_1row_1k/sqlite_delete | 828475.00 | 3.688 | 1.0192 | 0.9889 | 1.0042 |
| compare/durable_update_1row_1k/sqlite_wal | 273274.75 | 4.898 | 1.0264 | 1.0155 | 0.9770 |
| compare/durable_delete_1row_1k/lin | 341723.00 | 11.490 | 1.0697 | 0.9899 | 0.9718 |
| compare/durable_delete_1row_1k/sqlite_delete | 851113.25 | 2.251 | 0.9872 | 0.9819 | 1.0006 |
| compare/durable_delete_1row_1k/sqlite_wal | 284727.00 | 4.080 | 0.9972 | 0.9905 | 0.9919 |
| compare/durable_commit_1row_1k/lin | 340489.25 | 1.726 | 0.9892 | 0.9940 | 0.9825 |
| compare/durable_commit_1row_1k/sqlite_delete | 867091.75 | 3.392 | 0.9950 | 0.9915 | 0.9964 |
| compare/durable_commit_1row_1k/sqlite_wal | 285037.75 | 3.803 | 0.9893 | 0.9808 | 0.9807 |
| compare/durable_rollback_1row_1k/lin | 37122.50 | 3.312 | 0.9995 | 1.0006 | 0.9904 |
| compare/durable_rollback_1row_1k/sqlite_delete | 89430.00 | 2.427 | 1.0009 | 1.0050 | 0.9927 |
| compare/durable_rollback_1row_1k/sqlite_wal | 14501.75 | 3.344 | 0.9965 | 0.9805 | 1.0033 |
| compare/durable_update_1row_10k/lin | 516114.00 | 3.003 | 1.0491 | 1.0326 | 0.9810 |
| compare/durable_update_1row_10k/sqlite_delete | 876849.25 | 5.456 | 1.0437 | 0.9908 | 0.9640 |
| compare/durable_update_1row_10k/sqlite_wal | 304619.75 | 4.818 | 0.9560 | 0.9883 | 1.0340 |
| compare/durable_delete_1row_10k/lin | 534132.00 | 5.464 | 0.9678 | 1.0219 | 1.0589 |
| compare/durable_delete_1row_10k/sqlite_delete | 922534.00 | 6.298 | 1.0041 | 1.0068 | 0.9726 |
| compare/durable_delete_1row_10k/sqlite_wal | 317945.00 | 4.861 | 0.9702 | 0.9625 | 0.9782 |
| compare/durable_commit_1row_10k/lin | 517243.00 | 1.924 | 1.0231 | 1.0112 | 1.0061 |
| compare/durable_commit_1row_10k/sqlite_delete | 878977.75 | 8.547 | 0.9922 | 1.0250 | 1.0201 |
| compare/durable_commit_1row_10k/sqlite_wal | 296250.00 | 0.352 | 1.0025 | 1.0394 | 1.0386 |
| compare/durable_rollback_1row_10k/lin | 46815.75 | 0.969 | 1.0135 | 1.0426 | 1.0311 |
| compare/durable_rollback_1row_10k/sqlite_delete | 116804.00 | 4.360 | 1.0527 | 1.0001 | 0.9744 |
| compare/durable_rollback_1row_10k/sqlite_wal | 21182.25 | 5.202 | 0.9858 | 0.9758 | 1.0517 |

## All-invocation traced phases

These records include fresh validation/calibration work as well as measurement. They are not paired with individual benchmark observations. Each aggregate is the median of six process-level medians. Share uses each invocation phase/whole-work time, then medians within/across processes. Checkpoint is an inclusive parent and inactive in these fixtures; never add active checkpoint to its nested phases.

| Case | Records | Whole-work ns | Payload bytes | Exec % | Encode % | CRC % | Write % | Sync % | Rollback % |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| compare/durable_update_1row_1k/lin | 66 | 335953.00 | 4112.0 | 5.714 | 1.699 | 0.172 | 5.064 | 85.763 | 0.000 |
| compare/durable_delete_1row_1k/lin | 66 | 345461.50 | 4112.0 | 7.324 | 1.685 | 0.175 | 4.989 | 83.800 | 0.000 |
| compare/durable_commit_1row_1k/lin | 66 | 333318.50 | 4112.0 | 5.522 | 1.704 | 0.194 | 4.108 | 86.336 | 0.000 |
| compare/durable_rollback_1row_1k/lin | 66 | 33602.50 | 0.0 | 51.957 | 0.000 | 0.000 | 0.000 | 0.000 | 34.183 |
| compare/durable_update_1row_10k/lin | 66 | 515467.50 | 4112.0 | 5.443 | 1.286 | 0.181 | 8.135 | 83.272 | 0.000 |
| compare/durable_delete_1row_10k/lin | 66 | 534096.00 | 4112.0 | 7.031 | 1.250 | 0.187 | 7.979 | 81.983 | 0.000 |
| compare/durable_commit_1row_10k/lin | 66 | 519209.50 | 4112.0 | 5.486 | 1.300 | 0.174 | 8.158 | 82.949 | 0.000 |
| compare/durable_rollback_1row_10k/lin | 66 | 48002.50 | 0.0 | 58.992 | 0.000 | 0.000 | 0.000 | 0.000 | 28.724 |

## Peer numeric comparisons

Both SQLite FULL DELETE/WAL retained; instrumented labels do not qualify the original binary.

| Case | Peer | Role | Lin/SQLite | Lin wins/6 |
|---|---|---|---:|---:|
| compare/durable_update_1row_1k/lin | sqlite_delete | original | 0.4051 | 6 |
| compare/durable_update_1row_1k/lin | sqlite_delete | control | 0.3977 | 6 |
| compare/durable_update_1row_1k/lin | sqlite_delete | disabled | 0.3859 | 6 |
| compare/durable_update_1row_1k/lin | sqlite_delete | traced | 0.3897 | 6 |
| compare/durable_update_1row_1k/lin | sqlite_wal | original | 1.2854 | 0 |
| compare/durable_update_1row_1k/lin | sqlite_wal | control | 1.2611 | 1 |
| compare/durable_update_1row_1k/lin | sqlite_wal | disabled | 1.1804 | 0 |
| compare/durable_update_1row_1k/lin | sqlite_wal | traced | 1.2352 | 0 |
| compare/durable_delete_1row_1k/lin | sqlite_delete | original | 0.4219 | 6 |
| compare/durable_delete_1row_1k/lin | sqlite_delete | control | 0.4286 | 6 |
| compare/durable_delete_1row_1k/lin | sqlite_delete | disabled | 0.4426 | 6 |
| compare/durable_delete_1row_1k/lin | sqlite_delete | traced | 0.4254 | 6 |
| compare/durable_delete_1row_1k/lin | sqlite_wal | original | 1.1707 | 0 |
| compare/durable_delete_1row_1k/lin | sqlite_wal | control | 1.2154 | 0 |
| compare/durable_delete_1row_1k/lin | sqlite_wal | disabled | 1.3211 | 0 |
| compare/durable_delete_1row_1k/lin | sqlite_wal | traced | 1.2464 | 0 |
| compare/durable_commit_1row_1k/lin | sqlite_delete | original | 0.3949 | 6 |
| compare/durable_commit_1row_1k/lin | sqlite_delete | control | 0.4230 | 6 |
| compare/durable_commit_1row_1k/lin | sqlite_delete | disabled | 0.3913 | 6 |
| compare/durable_commit_1row_1k/lin | sqlite_delete | traced | 0.3881 | 6 |
| compare/durable_commit_1row_1k/lin | sqlite_wal | original | 1.1940 | 0 |
| compare/durable_commit_1row_1k/lin | sqlite_wal | control | 1.2363 | 0 |
| compare/durable_commit_1row_1k/lin | sqlite_wal | disabled | 1.1730 | 0 |
| compare/durable_commit_1row_1k/lin | sqlite_wal | traced | 1.1849 | 0 |
| compare/durable_rollback_1row_1k/lin | sqlite_delete | original | 0.4018 | 6 |
| compare/durable_rollback_1row_1k/lin | sqlite_delete | control | 0.3597 | 6 |
| compare/durable_rollback_1row_1k/lin | sqlite_delete | disabled | 0.3643 | 6 |
| compare/durable_rollback_1row_1k/lin | sqlite_delete | traced | 0.3824 | 6 |
| compare/durable_rollback_1row_1k/lin | sqlite_wal | original | 2.5851 | 0 |
| compare/durable_rollback_1row_1k/lin | sqlite_wal | control | 2.3140 | 0 |
| compare/durable_rollback_1row_1k/lin | sqlite_wal | disabled | 2.3427 | 0 |
| compare/durable_rollback_1row_1k/lin | sqlite_wal | traced | 2.5120 | 0 |
| compare/durable_update_1row_10k/lin | sqlite_delete | original | 0.5785 | 6 |
| compare/durable_update_1row_10k/lin | sqlite_delete | control | 0.5623 | 6 |
| compare/durable_update_1row_10k/lin | sqlite_delete | disabled | 0.5840 | 6 |
| compare/durable_update_1row_10k/lin | sqlite_delete | traced | 0.5823 | 6 |
| compare/durable_update_1row_10k/lin | sqlite_wal | original | 1.6381 | 0 |
| compare/durable_update_1row_10k/lin | sqlite_wal | control | 1.6743 | 0 |
| compare/durable_update_1row_10k/lin | sqlite_wal | disabled | 1.8243 | 0 |
| compare/durable_update_1row_10k/lin | sqlite_wal | traced | 1.7446 | 0 |
| compare/durable_delete_1row_10k/lin | sqlite_delete | original | 0.5596 | 6 |
| compare/durable_delete_1row_10k/lin | sqlite_delete | control | 0.5728 | 6 |
| compare/durable_delete_1row_10k/lin | sqlite_delete | disabled | 0.5651 | 6 |
| compare/durable_delete_1row_10k/lin | sqlite_delete | traced | 0.5773 | 6 |
| compare/durable_delete_1row_10k/lin | sqlite_wal | original | 1.6781 | 0 |
| compare/durable_delete_1row_10k/lin | sqlite_wal | control | 1.8030 | 0 |
| compare/durable_delete_1row_10k/lin | sqlite_wal | disabled | 1.7005 | 0 |
| compare/durable_delete_1row_10k/lin | sqlite_wal | traced | 1.7922 | 0 |
| compare/durable_commit_1row_10k/lin | sqlite_delete | original | 0.5735 | 6 |
| compare/durable_commit_1row_10k/lin | sqlite_delete | control | 0.5466 | 6 |
| compare/durable_commit_1row_10k/lin | sqlite_delete | disabled | 0.5854 | 6 |
| compare/durable_commit_1row_10k/lin | sqlite_delete | traced | 0.5618 | 6 |
| compare/durable_commit_1row_10k/lin | sqlite_wal | original | 1.7297 | 0 |
| compare/durable_commit_1row_10k/lin | sqlite_wal | control | 1.6557 | 0 |
| compare/durable_commit_1row_10k/lin | sqlite_wal | disabled | 1.7222 | 0 |
| compare/durable_commit_1row_10k/lin | sqlite_wal | traced | 1.6910 | 0 |
| compare/durable_rollback_1row_10k/lin | sqlite_delete | original | 0.4207 | 6 |
| compare/durable_rollback_1row_10k/lin | sqlite_delete | control | 0.4635 | 6 |
| compare/durable_rollback_1row_10k/lin | sqlite_delete | disabled | 0.4027 | 6 |
| compare/durable_rollback_1row_10k/lin | sqlite_delete | traced | 0.4586 | 6 |
| compare/durable_rollback_1row_10k/lin | sqlite_wal | original | 2.2066 | 0 |
| compare/durable_rollback_1row_10k/lin | sqlite_wal | control | 2.4269 | 0 |
| compare/durable_rollback_1row_10k/lin | sqlite_wal | disabled | 2.4065 | 0 |
| compare/durable_rollback_1row_10k/lin | sqlite_wal | traced | 2.3627 | 0 |

## Whole-process peak RSS (KiB)

This includes fixture/oracle/reopen and buffered trace data; it is not isolated mutation memory.

| Case | Original | Control | Disabled | Traced |
|---|---:|---:|---:|---:|
| compare/durable_update_1row_1k/lin | 71632.0 | 71468.0 | 71462.0 | 71422.0 |
| compare/durable_update_1row_1k/sqlite_delete | 13000.0 | 13102.0 | 13264.0 | 13736.0 |
| compare/durable_update_1row_1k/sqlite_wal | 13162.0 | 13086.0 | 13362.0 | 13842.0 |
| compare/durable_delete_1row_1k/lin | 71442.0 | 71462.0 | 71462.0 | 71430.0 |
| compare/durable_delete_1row_1k/sqlite_delete | 13194.0 | 13200.0 | 13366.0 | 13992.0 |
| compare/durable_delete_1row_1k/sqlite_wal | 13096.0 | 13136.0 | 13214.0 | 13848.0 |
| compare/durable_commit_1row_1k/lin | 71490.0 | 71364.0 | 71378.0 | 71450.0 |
| compare/durable_commit_1row_1k/sqlite_delete | 13058.0 | 13080.0 | 13314.0 | 14000.0 |
| compare/durable_commit_1row_1k/sqlite_wal | 13214.0 | 13034.0 | 13412.0 | 14006.0 |
| compare/durable_rollback_1row_1k/lin | 71508.0 | 71438.0 | 71498.0 | 71520.0 |
| compare/durable_rollback_1row_1k/sqlite_delete | 13010.0 | 13098.0 | 13158.0 | 13812.0 |
| compare/durable_rollback_1row_1k/sqlite_wal | 13180.0 | 13216.0 | 13412.0 | 13922.0 |
| compare/durable_update_1row_10k/lin | 592272.0 | 592224.0 | 592322.0 | 592324.0 |
| compare/durable_update_1row_10k/sqlite_delete | 19614.0 | 19882.0 | 19776.0 | 20298.0 |
| compare/durable_update_1row_10k/sqlite_wal | 19662.0 | 19672.0 | 20082.0 | 20184.0 |
| compare/durable_delete_1row_10k/lin | 592090.0 | 591982.0 | 591986.0 | 592064.0 |
| compare/durable_delete_1row_10k/sqlite_delete | 19570.0 | 19670.0 | 19914.0 | 20168.0 |
| compare/durable_delete_1row_10k/sqlite_wal | 19640.0 | 19688.0 | 19858.0 | 20370.0 |
| compare/durable_commit_1row_10k/lin | 592320.0 | 592160.0 | 592274.0 | 592302.0 |
| compare/durable_commit_1row_10k/sqlite_delete | 19590.0 | 19804.0 | 19738.0 | 20360.0 |
| compare/durable_commit_1row_10k/sqlite_wal | 19512.0 | 19880.0 | 20092.0 | 20134.0 |
| compare/durable_rollback_1row_10k/lin | 592332.0 | 592214.0 | 592360.0 | 592320.0 |
| compare/durable_rollback_1row_10k/sqlite_delete | 19824.0 | 19880.0 | 20010.0 | 20254.0 |
| compare/durable_rollback_1row_10k/sqlite_wal | 19722.0 | 19922.0 | 19974.0 | 20396.0 |

Complete:576 processes,4608 measured observations,528 separately scoped invocation records. Full eight-peer objective remains unachieved.
