# Sparse hashing output hypothesis

Baseline a7d4b02c3. The calibrated embedding profile identifies Arc vector allocation/copy as the largest measured subphase at10k, with material observer overhead. This candidate does not assume allocation or copying alone caused prior regressions.

For occupancy touched.len()*5<dim, allocate the same full Arc<[f32]> output using new_uninit_slice, initialize EVERY element to valid positive f32 zero, then overwrite touched coordinates with already-normalized f32 values. Dense output keeps original Arc::from. Feature extraction, normalization order/fallback, input text, dimensions, custom Embedder batch overrides, scratch reset, API representation and retained ownership remain. No vector sparsification or feature omission. The single assume_init is safe because every element is initialized before conversion; no raw pointers or uninitialized reads. Panic before conversion drops MaybeUninit storage safely.

Correctness: dense-bit oracle, occupancy threshold both sides, zero/odd dimensions, large normalization fallback, scratch reset and held output independence. Existing dense-feature oracle includes ASCII controls/Unicode whitespace and several dimensions. Full release tests and a runtime mutant skipping a touched output coordinate are required; source must restore exactly.

Planned component: INSERT10k target; INSERT1k, point_get, filter_eq, UPDATE1k, scalar Full/Normal UPDATE, SQLite INSERT10k, DuckDB filter_eq controls. Two6-pair series, baseline/control same binary, rotating cases/roles,8observations/fresh process,max_iterations1,zero warmup:324processes/2592observations. Target eachseries6/6positive and median gain above median absolute A/A; controls gain>=-max(2%,A/A). Preserve all raw data; no unchanged rerun or relaxed threshold. Whole-process CPU/RSS separate from operation latency.

Passing this component would still require complete source/engine and original eight-peer qualification. Restored COW+CAS baseline has a failed full-source gate. No production adoption or performance win yet.
