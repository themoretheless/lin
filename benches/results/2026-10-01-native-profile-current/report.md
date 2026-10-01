# Current native bulk insert profile

Built the existing release profile_bulk example against current retained production changes. Sampled its main thread for 15 seconds at 1 ms. Workload completed 1305 fresh 10k-document batches in 20.012 seconds, with row count checks.

This is a sampling diagnostic, not a peer benchmark: it includes Db::empty, index setup, row-count validation and DB destruction; it uses the existing profile fixture with relative timestamps. Do not compare its total throughput to native benchmark timers.

The record_row call subtree is prominent (four direct stack groups: 1055, 890, 707 and 30 samples), with field/value allocation and BTree insertion beneath it. Embedding and FTS subtrees are also substantial. Raw sample.txt retained. Next experiment sorts insert record fields once during preparation and uses map bulk collection at execution, preserving duplicate last-value and runtime timestamps; evaluated in paired full native insert benchmarks separately.
