# Rejected scoped batch hashing embedding

Baseline main 2feb9a8, with retained parallel sparse WAL encoding. Baseline SHA256 was checked against the pinned final candidate metadata of that retained experiment. This experiment affects only HashingEmbedder::embed_batch: for at least 4096 texts and dimension at least 128, split the input into two halves, reuse scratch separately for each half, run the right half on one scoped worker and append its results after the left half. Thread creation failure falls back to serial processing; worker panics propagate. Third-party embedders and the Embedder trait defaults are unchanged. No pool or dependency was added, and worker creation/join and merging are included in insertion timing. Additional CPU concurrency and per-thread scratch/thread resources are part of the attempted change.

The first candidate embeds scoped setup within the trait method; the second puts the same parallel work behind a separate #[inline(never)] private function while retaining the existing small-batch loop. Both are rejected. The second candidate tested a code-structure hypothesis, not a proven cause for the first regression. These results do not establish whether thread startup, allocator contention, code layout or other factors caused the slowdown.

## Six-pair measurements for each candidate

For each variant: six alternating process pairs on both native and durable fixtures, 24 fresh samples per case, one operation/sample, no warmup. Exact existing fixture validation is outside timing. Baseline and candidate executables were pinned; builds/tests did not overlap timing. SQLite is an unchanged control. Gains are median paired improvements from six process medians; negative means slower. CPU frequency/background load are uncontrolled.

| Variant and mode | Lin 1k | SQLite control 1k | Lin 10k | SQLite control 10k |
|---|---:|---:|---:|---:|
| Initial scoped branch Native | -3.763%, 0/6 | +1.073%, 5/6 | -4.435%, 0/6 | -1.035%, 1/6 |
| Initial scoped branch Durable | -4.458%, 0/6 | -0.997%, 2/6 | -7.149%, 0/6 | +0.229%, 4/6 |
| Outlined scoped function Native | -5.808%, 0/6 | -0.831%, 1/6 | -7.604%, 1/6 | -0.846%, 2/6 |
| Outlined scoped function Durable | -4.596%, 0/6 | +2.154%, 4/6 | -5.805%, 0/6 | -1.396%, 2/6 |

Lin consistently regressed on both sizes and both execution modes, with at most one win in any scenario. SQLite's smaller control shifts do not account for all of the Lin differences. Small batches never create the worker but also regress; no assembly/profile attribution was made. Do not retain either implementation or infer a win from split computation alone.

## Validation and restoration

Seven focused embedding tests passed for each candidate. Added coverage tests 4095/4096/4101 texts at dimensions 8/768, output order/length and exact float bits against individual embedding calls, contextual Greek and other Unicode, vertical-tab/Unicode whitespace, empty texts, and injected WouldBlock worker-creation fallback. The outlined variant additionally asserts each vector dimension. Existing independent dense-reference, sparse normalization and exhaustive bigram hash tests passed. Full workspace was not rerun because neither production variant is retained.

src/embed.rs was restored byte-for-byte from before-embed.rs; the added tests were restored too. Production behavior and all earlier retained improvements remain unchanged. Candidate snapshots, separate binary/source hashes, environment, test/build logs, run scripts and raw outputs identify both attempts. The full named-peer objective and durable SQLite gaps remain unresolved.
