# Vector finish breakdown: diagnostic protocol

Frozen b9f10bc776b05eb5235237e9a900c2ef6aaed1fd ordinary default runtime. Prior diagnostic run 41635273783 measured finish at 4.17–4.30 microseconds per row, versus feature fill 0.34–0.39. This follow-up separates normalization, Arc allocation/copy, and touched-slot cleanup. Only a temporary patch adds three per-finish timers and chunk-local accumulated counters to Scratch. Original source is restored before any timing. Extra fields change Scratch layout; instrumentation changes code generation and overhead. No exact allocator attribution, wall-time fractions or speedup claim is allowed.

Six processes / 18 wall observations at 1k, 10k and 100k; unchanged full row validation, embeddings, FTS and index maintenance. Ordinary and diagnostic binaries are built with explicit package clean, color-free verbose compilation, locked dependencies and no build-profile changes. Exact source/patch/harness/binary hashes and complete values are retained. Independent auditor reconstructs the patch, checks all artifact members and derives aggregates from stderr, requiring exactly 3*n embedded rows. Linux evidence is pending. No rejected candidate is retried.

Local instrumented release embedding tests: 11 passed, zero failed. Patch reconstruction passed and runtime source restored byte-for-byte.
