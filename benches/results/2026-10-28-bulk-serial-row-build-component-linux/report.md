# Serial bulk row construction: rejected

[Run40629229553](https://github.com/themoretheless/lin/actions/runs/40629229553), frozen source `97217ca461883f4f34550acbb599a7c6e698e056`. Independent frozen-source/raw audit passed for all324fresh processes /2592observations.449release tests /2ignored,18correctness smokes and dropped-tail runtime mutant passed. All candidate source files other than exec.rs match baseline; the candidate exec.rs differs only by the declared serial row construction and test-only legacy helper.

| Case | Series | Median gain | Median absolute A/A | Positive pairs | Pass |
|---|---|---:|---:|---:|---|
| compare/insert_bulk_10k/lin | series1 | -0.2807% | 1.5096% | 3/6 | False |
| compare/insert_bulk_10k/lin | series2 | -5.3357% | 2.6350% | 0/6 | False |
| compare/insert_bulk_1k/lin | series1 | -2.5249% | 1.0558% | 1/6 | False |
| compare/filter_eq/lin | series2 | -3.8945% | 1.8667% | 1/6 | False |

Both target windows and two control windows failed. No reliable speedup established. Removing the row-build worker did not validate the proposed way to recover bulk INSERT performance. This separate-source experiment does not identify the cause of the earlier parallel hashing regression. The unchanged INSERT1k and filter_eq losses have no proven unique cause.

Whole-process INSERT10k resource medians:

| Role | User CPU seconds | System CPU seconds | Peak RSS KiB |
|---|---:|---:|---:|
| baseline | 0.715 | 0.04 | 78676.0 |
| control | 0.7 | 0.04 | 78672.0 |
| candidate | 0.7 | 0.03 | 78676.0 |

Resource scope includes setup, validation, all operations and destruction of each fresh process; not isolated operation cost. Lower system CPU does not override the failed latency gate or prove causality. No RSS improvement claimed.

All2067artifact files are preserved in raw-artifact.tar.gz; every member and archive SHA256 are verified in raw-manifest.json. Raw observations are untrimmed. Independently recomputed values and source provenance are in frozen-audit.json.

Decision: reject candidate; restore exec.rs byte-for-byte to a7d4b02c3. No unchanged rerun, production adoption, complete-engine qualification or eight-peer superiority claim. Candidate source/tests/evidence remain recoverable. The restored COW+CAS baseline still has a failed full-source gate.
