# Sparse hashing output: component gate rejected

[Run40634605768](https://github.com/themoretheless/lin/actions/runs/40634605768), frozen source6f0b233455684c15a1613a06eee33157986bde03. Independent audit passed for all324fresh processes /2592observations,450release tests /2ignored,18correctness smokes, exact source scope and missing-coordinate runtime mutant. Normalization, full dimensions, bit/order semantics and custom embedder paths retained.

| Case | Series | Median gain | Median absolute A/A | Positive pairs | Pass |
|---|---|---:|---:|---:|---|
| compare/insert_bulk_10k/lin | series1 | 7.7903% | 2.0806% | 5/6 | False |
| compare/insert_bulk_10k/lin | series2 | 8.0544% | 1.4034% | 6/6 | True |

All16control windows passed their frozen noise-relative threshold. The target passed series2 but failed series1 because only5/6pairs were positive. Round4 baseline17707303ns/control18136463.5ns/candidate17714153ns, gain-0.0386846%. The pair is retained with every observation; no unique cause established. Positive medians and a small loss do not waive the predeclared6/6criterion.

Whole-process INSERT10k resource medians:

| Role | User CPU seconds | System CPU seconds | Peak RSS KiB |
|---|---:|---:|---:|
| baseline | 0.57 | 0.03 | 79334.0 |
| control | 0.56 | 0.04 | 79402.0 |
| candidate | 0.55 | 0.035 | 79578.0 |

These resources include setup, validation, all operations and destruction; not isolated operation cost. No RSS improvement claimed. All2067artifact files retained in raw-artifact.tar.gz, every member and archive SHA256 verified in raw-manifest.json; unpacked raw data retained locally.

Decision: do not accept this candidate or rerun it unchanged. Restore embed.rs byte-for-byte to a7d4b02c3; source/tests/proofs remain recoverable from frozen Git and archive. Median gains make output materialization a useful changed-hypothesis direction, but source/full-engine/eight-peer qualification is absent. Next experiments must change the mechanism and preserve all data and thresholds. Production main unchanged; baseline COW+CAS full-source gate still failed.
