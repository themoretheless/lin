# Compact undo: mixed success/rollback CPU result

Main205 vs compact-undo-only prototype. Release macOS ARM64, six rotated A/A/B rounds, 18 processes, 1k/10k/100k fixtures. 16 samples each: 512 alternating prepared CAS updates followed by one explicit transaction UPDATE+rollback. Exact affected count, restored wing, index count and current generation behavior checked outside timing. Setup/drop of fixtures excluded; Handle drop and undo destruction/restore included. Production sources restored exactly.

| Rows | UPDATE gain | UPDATE A/A | Positive | Rollback gain | Rollback A/A | Positive |
|---:|---:|---:|---:|---:|---:|---:|
| 1000 | +4.376% | 1.105% | 5/6 | -5.796% | 4.342% | 2/6 |
| 10000 | +5.324% | 2.146% | 6/6 | -2.878% | 3.696% | 1/6 |
| 100000 | +2.724% | 1.959% | 5/6 | -3.433% | 5.668% | 1/6 |

Not accepted. Rollback1k regression exceeds A/A and UPDATE does not win6/6 at all sizes. Other rollback regressions remain recorded even when within A/A. Compact snapshot conversion cannot be treated as free just because it executes on rollback. Do not launch unchanged broad qualification or omit rollback to obtain acceptance.

Initial harness incorrectly expected rollback to reset generation; existing baseline advances generation during transaction run and keeps that value on explicit rollback. Failure artifact preserved, check corrected to baseline behavior before measuring candidate. No engine semantics changed to satisfy the harness.

This is repeated-row CPU diagnostic, not fresh-fixture Linux, Full durable or peer proof. Positive standalone row-key lifecycle results and nine saved allocations do not establish whole-database acceleration. Next design must avoid reconstructing full String-key row on rollback or otherwise demonstrate restored performance without weakening rollback semantics or journal accounting.
