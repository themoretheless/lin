# Bounded hashing-token slot cache: unqualified prototype

Only src/embed.rs changes relative to frozen b9f10bc776b05eb5235237e9a900c2ef6aaed1fd. Cache up to64 alphabetic ASCII words of3..32 bytes in each existing TLS Scratch, preserving ordered feature slots and weights (unigram1 then each trigram0.5). Numeric/mixed, Unicode, long and saturated-cache misses use the original feature hash path. Reset dimension-specific cached positions on dimension changes. No vector caching, text skipping, dimensionality/model/API change, disabled features or weakened durability.

A cache hit hashes the word once for lookup and avoids repeated trigram hash/modulo work. It still applies every ordered feature addition. Memory is bounded by64 keys of at most32 bytes and at most31 usize slots each, plus bounded map/vector metadata; thread lifetime controls retention. This additional memory and short-word overhead require actual lifecycle/RSS and end-to-end checks; no speedup is claimed.

All12 embedder tests passed, including independent dense reference across dimensions and Unicode/whitespace cases, raw accumulator/final f32 bit comparison after64-entry saturation and dimension transitions, batch/single consistency and ANN tests. Workspace and performance qualification remain pending. Tests/report are not evidence of beating any peer. Prior rejected shape prototypes remain rejected.
