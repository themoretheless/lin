# Detailed INSERT phase discovery

Source 3401bc17de5651db425c88defa59ab9b03c5f6e4, default release worker on local macOS ARM64. Diagnostic timer patch only; original exec.rs restored byte-for-byte before the next ordinary build. Three fresh samples at 1k, 10k and 100k; the latter executes ten 10k chunks. Exact worker readback passed for every sample. No peer timing, A/A, Linux qualification or speedup claim.

For the 33 observed 10k chunks, median microseconds: embedding text assembly 666, batch vector generation 1194, vector attachment 312; capacity reservation 5, secondary index maintenance 1332, FTS 1327, HNSW 0 (no index configured by this workload), row maps 588. All raw observations and the diagnostic patch are preserved. The diagnostic executable remains `/tmp/lin-insert-detailed-worker-20261103`.

FTS fresh append currently uses Unicode whitespace decoding even for ordinary ASCII documents. A prospective candidate can specialize ASCII token splitting, preserving vertical-tab semantics and Unicode fallback; this is only a hypothesis until tested and measured against the ordinary default binary.
