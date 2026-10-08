# Project Memory Index

## Optimization Documentation
- [Partial Indexes](partial-indexes.md) — WHERE predicates for selective index population with memory/performance savings
- [Content Hash Caching](content-hash-caching.md) — thread-local memoization of FNV-1a hashes to avoid redundant computations on inserts/updates  
- [Cache-Friendly Layout](cache-friendly-layout.md) — SoA columnar arrays and fast O(1) count tracking via col_counts

## Implementation Patterns
- [Feature Flags Pattern](feature-flags.md) — parallel, compress-flate, compress-lz4, compress-wal gated optional dependencies
- [Zero-Copy Cell Patterns](zero-copy-cells.md) — clone_for_borrow() preserves Arc<T> references throughout exec/index/cold paths
