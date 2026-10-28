# Parallel hashing batch: rejected

Run [40626189213](https://github.com/themoretheless/lin/actions/runs/40626189213), frozen source `60bd5deec992439c7d4f47dfae6903c835da8202`. Independent frozen-source/raw audit passed: 324 fresh processes /2592 observations, 452 release tests /2 ignored, 18 correctness smokes, runtime order mutant rejected, source restored. Rust reported four available CPUs; this proves host eligibility, not per-operation worker-spawn success.

Both target windows failed:

| Case | Series | Median gain | Median absolute A/A | Positive pairs | Pass |
|---|---|---:|---:|---:|---|
| compare/insert_bulk_10k/lin | series1 | -33.8900% | 0.6564% | 0/6 | False |
| compare/insert_bulk_10k/lin | series2 | -32.9354% | 1.4020% | 0/6 | False |
| compare/wal_scalar_normal_update/lin | series2 | -4.8763% | 1.4619% | 2/6 | False |

All observations, including losses, were retained. There is no evidence of a reliable speedup. The unchanged Normal UPDATE control failure has no proven unique cause; it does not explain away the bulk regression.

Whole-process resource medians for bulk INSERT10k:

| Role | User CPU seconds | System CPU seconds | Peak RSS KiB |
|---|---:|---:|---:|
| baseline | 0.68 | 0.035 | 78684.0 |
| control | 0.69 | 0.03 | 78708.0 |
| candidate | 0.7 | 0.15 | 78564.0 |

Resource measurements include setup, validation, all operations and destruction in each fresh process. They are not isolated operation CPU/RSS measurements. The higher system CPU accompanies the regression but does not prove its cause. No memory improvement is claimed from these medians.

`raw-artifact.tar.gz` preserves all 2064 artifact files, with every member and the archive verified by SHA256 in `raw-manifest.json`. `frozen-audit.json` independently recomputes the complete matrix and resource medians. Extracted raw files remain locally available.

Decision: reject this candidate and restore `src/embed.rs` byte-for-byte to baseline `a7d4b02c3`. Candidate source and its tests remain recoverable from frozen Git and the archive. No unchanged rerun, production adoption, full-engine qualification or eight-peer superiority claim. The COW+CAS baseline itself still has a failed complete-source gate. Future work must test a changed, evidence-supported hypothesis and preserve the original comparison scope.
