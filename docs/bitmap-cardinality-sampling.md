# Bitmap cardinality sampling during slab insertion

For an index with `bitmap_card_field`, slab insertion estimates distinct values using the first at most 100 input rows. That sample is now computed once per nonempty filtered insertion rather than reconstructed for every row. It includes the same original input rows as before, including rows excluded by a partial predicate.

Representation selection is unchanged: a boolean in the sample enables bitmap; a boolean on the current row also enables bitmap even outside the sample. Otherwise a nonempty sample must have at most 16 distinct values, or 32 when the current forward map has more than 10000 keys. The forward-map threshold is evaluated per row, so crossing 10000 during a batch retains previous behavior. An empty/missing-value sample retains standard postings.

No new default bitmap enablement, public API, persistent cache or disk format is introduced. Key extraction, unique failures, partial predicates, posting insertion and reverse-map semantics remain the original contracts. Ordinary indexes without the flag do not build a sample.

[Qualification](../benches/results/2026-10-24-bitmap-sample-once/report.md) includes comparison against the original algorithm, threshold/boolean/partial/unique boundaries, full workspace checks, actual index-operation timings, ordinary native/durable regression observations and current available-peer refresh. The 16–18× observed improvement concerns sampling-enabled index insertion, not whole-Db or universal peer performance. Durable SQLite 1k and live MSSQL/Kusto comparisons remain unresolved.
