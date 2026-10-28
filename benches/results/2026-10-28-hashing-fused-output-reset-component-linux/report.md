# Fused sparse output/reset: rejected

[Run40637176096](https://github.com/themoretheless/lin/actions/runs/40637176096), frozen source0ea6dcc38010d69648ef938e1241b1b861f82626. Independent audit passed for all324fresh processes/2592observations,450release tests/2ignored,18correctness smokes,exact source scope and all3runtime mutants (missing coordinate,stale value,stale seen). Every source restored after mutation/build overlays.

| Case | Series | Median gain | Median absolute A/A | Positive pairs | Pass |
|---|---|---:|---:|---:|---|
| compare/insert_bulk_10k/lin | series1 | 0.8210% | 1.0093% | 4/6 | False |
| compare/insert_bulk_10k/lin | series2 | 0.0836% | 2.0368% | 3/6 | False |
| compare/filter_eq/duckdb | series1 | -3.6777% | 3.1288% | 2/6 | False |

Both INSERT10k target windows failed; their positive medians are below measured A/A variability. One unchanged DuckDB filter_eq control also failed; it has no proven unique cause and does not override the target failures. No reliable speedup established. This run did not directly pair against preceding sparse-output candidate2c8515692; differences from that separate run do not establish an incremental fusion effect or its cause.

Whole-process INSERT10k resource medians:

| Role | User CPU seconds | System CPU seconds | Peak RSS KiB |
|---|---:|---:|---:|
| baseline | 0.665 | 0.03 | 78624.0 |
| control | 0.66 | 0.03 | 78674.0 |
| candidate | 0.67 | 0.03 | 78818.0 |

Resources include setup,validation,all operations and destruction;not isolated operation cost. No RSS/CPU improvement claimed. All2071artifact files are retained in raw-artifact.tar.gz with every member and archive SHA256 verified; raw data untrimmed and locally unpacked.

Decision: reject and restore embed.rs byte-for-byte to a7d4b02c3. Candidate/tests/three mutant proofs remain recoverable from Git and archive. No unchanged rerun,gate relaxation,production adoption,full-engine qualification or eight-peer superiority claim. Restored COW+CAS baseline still has a failed complete-source gate.
