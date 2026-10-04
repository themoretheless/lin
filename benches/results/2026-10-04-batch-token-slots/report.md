# Rejected bounded batch token-slot cache

Baseline main 4926d9c. Candidate: for HashingEmbedder batches of at least 128 texts, keep up to 32 ASCII alphabetic tokens of byte length 3..24 in a per-batch FxHashMap. Cache the exact ordered unigram/trigram slot sequence, including repeated slots, instead of aggregated weights. On each occurrence apply weight 1 to the unigram slot, then 0.5 to every trigram slot in the original order. Single embedding, numbers, shorter/longer and Unicode tokens keep the existing hashing path. Whole-string bigrams and normalization are unchanged. Cache contents do not outlive a batch or cross embedder dimensions. This adds bounded maps/keys/slot arrays and lookups; no CPU worker or dependency is added.

## Native and durable comparison

Two independent series using the same pinned baseline/candidate binaries, each six alternating process pairs in both modes. Each case has 24 fresh checked fixtures, one operation/sample, no warmup. Existing row/count validation is outside timing. Builds/tests did not overlap measurements. SQLite is an unchanged control. Percentages are medians of the six paired percentage changes from per-process medians, not aggregate-median ratios. CPU frequency/background load are uncontrolled.

| Series/mode | Lin 1k | SQLite control 1k | Lin 10k | SQLite control 10k |
|---|---:|---:|---:|---:|
| First series Native | +1.093%, 3/6 | -0.303%, 3/6 | +11.699%, 5/6 | +1.605%, 4/6 |
| First series Durable | -2.152%, 3/6 | +0.282%, 4/6 | -5.075%, 3/6 | -7.529%, 2/6 |
| Independent repeat Native | -0.699%, 1/6 | -0.475%, 2/6 | -1.266%, 0/6 | -0.151%, 3/6 |
| Independent repeat Durable | -4.336%, 2/6 | +0.739%, 4/6 | +0.609%, 6/6 | +1.088%, 5/6 |

The initial native 10k gain does not reproduce: the same candidate loses every native 10k pair in the repeat. The first durable 10k series has large unchanged-control slowdown, so its result is not an isolated causal estimate. Durable 1k is negative in both series, with a repeat 4.336% slowdown against a slightly improving SQLite control. Reject the cache: exact bit compatibility does not establish a useful or consistently faster implementation. No assembler/profile attribution or universal caching claim is made.

## Validation and restoration

All seven focused embedding tests passed. New coverage asserts the 32-entry cap after 64 distinct eligible tokens; raw accumulator bits versus original ordered token updates; the 127/128/131 batch boundary; exact output length/dimension and float bits against individual original embedding; dimensions 8/9/768/1536; repeated eligible tokens, empty strings, Greek contextual lowercasing and other Unicode, ASCII vertical tab, Unicode whitespace, numeric and too-long tokens. Existing independent dense-reference, normalization-boundary and exhaustive bigram hash tests passed. Full workspace was not rerun because the production candidate and new tests were restored byte-for-byte to before-embed.rs.

Baseline binary SHA256 matches the retained sparse-classifier candidate metadata. Raw results, source/binary hashes, test/build logs and invocation scripts identify both measurements. Prior confirmed WAL improvements remain in production, including the small local durable-10k lead over SQLite; durable/native 1k and full named-peer proof remain unresolved. The goal stays active.
