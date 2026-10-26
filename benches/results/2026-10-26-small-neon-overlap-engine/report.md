# Small NEON WAL overlap — initial policy rejected

2026-10-26, baseline aa8f307f. This candidate extends the existing shared encoder to512..4095 rows with256Ki finalized embedding floats;4096..16384 retains its2Mi minimum. Upper16Mi float budget,implicit single durable statement,no-edge policy,codec/checksum/Full sync remain unchanged. Large low-dimensional batches are explicitly kept serial. Source delta only exec.rs and cfg(test) tests. pre-narrowing-candidate-exec.rs is a premeasurement draft with a broader float gate; no benchmark observations use that draft.

Default workspace and no-default/GPU/parallel/compression checks passed. Readonly replay/failure fallbacks cover512/1000/4096; quota/index precedence checks run at512, large64-dimensional guard stays serial.18 process-exit writers and18 independent readonly verifiers cover512/1k/10k ×1/3/4 commits; WAL bytes equal baseline/candidate in all9 scenarios. Exact release binary hashes and source snapshots retained. Functional success does not establish performance acceptance.

## Conventional pairs

Two series ×6 alternating process pairs/native+Full durable,24 one-operation samples/case,48 processes/4608 observations. Setup/preparation/checks/destruction outside timing; embeddings/FTS/indexes and sync preserved. All exit0. No own build/test overlaps timing; host load retained.

| Case | Lin gain series1 /2 | Positive pairs series1 /2 | SQLite control gain series1 /2 |
|---|---:|---:|---:|
| native1k |−1.388% /−2.019% |1/6 /0/6 |+1.198% /+0.261% |
| native10k |−1.188% /+0.244% |3/6 /3/6 |+0.828% /+0.167% |
| durable1k |+11.386% /+10.716% |6/6 /4/6 |+0.966% /+2.641% |
| durable10k |+0.041% /−2.757% |3/6 /1/6 |+0.193% /−0.387% |

Candidate durable1k medians1.658/1.666ms still lose SQLite1.428/1.426ms. Native conventional shifts are retained; do not assume the disabled path is performance-invariant just from source inspection.

## Interleaved calibration

Each row below is6 process pairs/size with24 samples/worker,exact nine-field/f32-bit readback. Two full durable series,two actual511/512 boundary series,then two separately built native-control series.288 workers/6912 measured operations plus288 preflights. A/A uses byte-identical baseline copies; all workers exit0 and owned fixture cleanup/source-drift assertions pass. These protocols are not pooled with conventional pairs.

| Path /rows | A/B gain series1 /2 | Positive pairs series1 /2 | A/A absolute difference series1 /2 |
|---|---:|---:|---:|
| durable1k |+12.235% /+11.455% |6/6 /6/6 |1.362% /0.673% |
| durable10k |−1.385% /+1.001% |0/6 /5/6 |0.615% /2.090% |
| durable511 guard |−1.690% /−1.280% |1/6 /1/6 |2.032% /1.516% |
| durable512 |+6.253% /−5.197% |6/6 /2/6 |1.799% /1.068% |
| native1k |+0.964% /+0.814% |6/6 /5/6 |0.787% /1.156% |
| native10k |−0.264% /+0.908% |2/6 /4/6 |1.164% /2.746% |

Native interleaved does not reproduce the conventional small native regression; no causal attribution to compiler layout,thread startup,scheduling or caches is made. Durable10k shifts are mixed. The decisive failure is512: second-series pairs5/6 lose50.988% /28.094%, all preserved. Do not remove these observations because1k improves.

## Decision

**Reject this512-row/256Ki-float policy; do not merge it.**1k gain is promising and reproducible,but the proposed lower boundary fails the second full-engine calibration. CPU cold/warm results did not qualify that boundary. Preserve raw successes/losses,RSS,byte-equal WAL,functional tests and both candidate snapshots. A separate candidate can require more actual vector work (512Ki floats for small batches), with new682/683 default-dimension boundary controls and unchanged original large-batch gate; it must qualify independently.

No production change from this candidate, no fresh all-peer win, no physical power-loss guarantee. The broad objective remains unmet: durable1k SQLite gap,native100k outlier,missing Kusto and incomplete real Microsoft qualification remain separate obligations.
