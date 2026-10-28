# Native bulk profiling interpretation

Run40622638028,frozen0ea1c38c3,profiles COW+CAS sourcea7d4b02c3. All48fresh processes/384wall observations and132trace invocations retained. Independent frozen-source/raw audit passed;449instrumented release tests/2ignored and6Linux full-value/phase-count smokes passed. Overlays remove exactly tooriginal code after stripping17clock-hook lines. No optimization/source acceptance orpeer victory is qualified.

At1k/10krows,respectively:embedding32.286%/40.666%,scalar-index/reserve21.203%/12.186%,FTS14.218%/15.464%,row construction15.565%/11.190%,identity/hash/FK10.536%/12.595%,rowmaps5.793%/7.357%. Final Store installation is0.080%/0.081%. These are same-invocation caller-wall fractions aggregated within then across processes. Independent medians are not forced tosum100%.

Observer calibration:A/A0.355%/1.969%;disabled/original0.99709/1.00060;traced/disabled0.99838/1.00103. The clock instrumentation perturbs the measured path;these ratios record its bounded observation overhead,not an optimization gain. Trace records include validation/calibration anddo not align1:1withthe8measured observations perprocess.

This profiles current COW only;it doesnot compare BTreeMap phases anddoesnot establish the cause oftheprior5–8%bulk regression. Itdoesidentify large current costs suitable for a distinct candidate. The input Db teardown remains outside the bulk timer;Store installation is measured butnot dominant. No missing field/WAL/durability work was removed.

Next hypothesis:batch execution inside thebuiltin deterministic HashingEmbedder. Current embed_batch walks everytext serially withthread-local scratch. A scoped second worker for sufficiently large batches could reduce this32–41%wall fraction while preserving order andeveryoutputfloatbit. Itmust not parallelize custom backend callbacks,precompute embeddings outside execution,skip existing vector payloads,orchange normalization/dimensions. Worker-spawn failure needs serial fallback. Exact sequential/parallel vector tests andfull source/control qualification are required before adoption. No speedup is yet established.

Every raw file,source/build/test/smoke/trace asset is inraw-artifact.tar.gz;allarchive member hashes verified. The full eight-peer objective remains incomplete.
