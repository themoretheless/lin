# Original-row undo ownership: Linux qualification not accepted

Frozen6c96cdbd, run40401845440. All36 A/A/B processes and all candidate/contract tests passed; performance evaluation exited3. Raw20cases×8samples per process,5760timed operations, independently recomputed240case-pairs. No observations dropped.

| Lin target | Series | Gain | Absolute A/A | Positive pairs |
|---|---|---:|---:|---:|
| compare/update_1row_1k/lin | series1 | 8.969% | 2.574% | 6/6 |
| compare/update_1row_1k/lin | series2 | 8.422% | 1.264% | 6/6 |
| compare/update_1row_10k/lin | series1 | 4.987% | 2.109% | 4/6 |
| compare/update_1row_10k/lin | series2 | 1.884% | 2.493% | 6/6 |
| compare/update_1row_100k/lin | series1 | 1.604% | 3.722% | 5/6 |
| compare/update_1row_100k/lin | series2 | 4.845% | 2.180% | 6/6 |

Update1k wins12/12 and exceeds A/A in both series, but the complete predeclared gate fails: update10k series1 wins4/6; series2 gain1.884% is below A/A2.493%. Update100k series1 gain1.604% is below A/A3.722% and wins5/6. Full controls and failed conditions retained in artifact/paired-report.md and verification.json. All120 same-case candidate comparisons lose to SQLite. Do not declare the 1k result a broad accepted rollout or a closed SQLite gap.

Production source unchanged. The allocation reduction is real in its counting contract; the full-engine gain is insufficiently qualified across target sizes. Next distinct hypothesis is bitmap emptiness checks during index edits, based on source/profile: replacing popcount-of-every-word with early emptiness detection. No format/layout or durability-policy change is needed; test it separately before acceptance.
