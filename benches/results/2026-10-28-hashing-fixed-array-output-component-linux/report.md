# Safe fixed-array hashing output: rejected

[Run40640126706](https://github.com/themoretheless/lin/actions/runs/40640126706), frozen sourced021bf29a21f1b91a626153063c6f1fa3df1aa98. Independent audit passed for all324fresh processes/2592observations,450release tests/2ignored,18correctness smokes,exact source scope and missing-coordinate runtime mutant. Full vectors/bit/order semantics retained; candidate embed.rs contains no unsafe/MaybeUninit. Functional safety does not establish performance.

| Case | Series | Median gain | Median absolute A/A | Positive pairs | Pass |
|---|---|---:|---:|---:|---|
| compare/insert_bulk_10k/lin | series1 | -1.3432% | 0.6561% | 0/6 | False |
| compare/insert_bulk_10k/lin | series2 | -1.5664% | 1.9719% | 1/6 | False |
| compare/wal_scalar_full_update/lin | series2 | -11.2605% | 4.1407% | 1/6 | False |

Both INSERT10k target windows failed; only1of12pairs was positive. Scalar Full UPDATE control series2 also failed. No unique cause established for that control; it does not override the target losses. This separate run does not establish an incremental comparison with prior sparse/fused output sources.

Whole-process INSERT10k resource medians:

| Role | User CPU seconds | System CPU seconds | Peak RSS KiB |
|---|---:|---:|---:|
| baseline | 0.71 | 0.04 | 79198.0 |
| control | 0.705 | 0.04 | 79260.0 |
| candidate | 0.715 | 0.04 | 79332.0 |

Resources include setup,validation,all operations and destruction;not isolated operation cost. No CPU/RSS improvement claimed. All2067artifact files retained in raw-artifact.tar.gz with every member and archive SHA256 verified in raw-manifest.json;raw observations untrimmed and unpacked locally.

Decision: reject and restore embed.rs byte-for-byte to a7d4b02c3. Candidate/tests/compatibility/mutant proofs remain recoverable. No unchanged rerun,gate relaxation,production adoption,complete-source qualification or eight-peer superiority claim. Restored COW+CAS baseline still has a failed full-source gate. Neither a safer initializer nor a compile-time-known allocation layout qualified the intended performance improvement.
