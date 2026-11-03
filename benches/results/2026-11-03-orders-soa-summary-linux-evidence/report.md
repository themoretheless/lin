# Linux aggregate source gate: passed, full objective open

[Run41563263817](https://github.com/themoretheless/lin/actions/runs/41563263817), frozen candidate `f37a85ff297b3771f6305c3561b9e1b507555999`. Native Linux x86_64, Rust1.96.0/default features. Candidate, correctness-aligned baselinea28aa758b and original production0dab1a9a4 were built ordinarily before timing; baseline/control share one exact executable. Full default Linux workspace passed before measurements.

Independent raw reconstruction verified144processes/1008case observations/21888wall samples: **24/24 target windows and18/18control windows pass** against the correctness-aligned baseline. All original thresholds, process pairs and samples are retained. Full returned read values/multiplicity and native six-field digest were reconstructed independently. All1019 uploaded raw members were read and SHA256-verified; all three exact64-bit x86_64 little-endian ELF executables were downloaded and independently rehashed against their complete source closures. Their hashes/sizes and original files are archived here.

Source improvements: per-mutation-epoch exact scalar SUM/numeric eligibility summaries, bounded exact comparison fallback, borrowed cached text group IDs, and single-lookup generic grouped COUNT. Actual quota and source-order regression tests remain active. First-query scans, orders-write latency, RSS, FULL WAL and the whole-source/peer objective are not qualified by this warm component.

## Original production is an additional comparison, not the acceptance reference

The original production source fails quota, floating-order and >2^53 predicate counterexamples. It still has faster results in some matched integer fixtures; those costs are retained:

|Case|Rows|Candidate paired gain vs production, series1/2 %|Positive pairs vs production|
|---|---:|---:|---|
|sum_by|1k|1.513 /1.580|5/6,5/6|
|sum_by|10k|35.328 /35.049|6/6,6/6|
|sum_by|100k|-18.970 /-10.087|0/6,0/6|
|sum_by_filtered|1k|-5.169 /-3.661|0/6,0/6|
|sum_by_filtered|10k|-2.552 /-3.236|0/6,0/6|
|sum_by_filtered|100k|-5.477 /-5.992|1/6,0/6|

Scalar SUM production-relative gains are81.216–99.714%; grouped COUNT13.316–25.422%. These do not imply engine-wide acceleration or peer wins. The correctness-aligned source gate passes, but original-production filtered/large-group regressions remain open optimization work. No prior failure is reclassified and no unchanged numerical source retry was performed.

The separate first six-peer comparison for this runtime is run41567995224. Its initial infrastructure-only attempt41567493710 has zero peer timings and remains archived. MSSQL evidence for0dab is private and is not transferred to this candidate; Kusto is absent. Production main is not changed by this evidence archive.
