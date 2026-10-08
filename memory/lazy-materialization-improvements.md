# Lazy Materialization Improvements

## Overview

Enhance existing ColdCol mmap approach with prefetch-based loading, hybrid warm/cold partitioning, and async background loading for frequently accessed data.

## Problem Statement

Current cold storage uses simple lazy materialization:
- Full collection decompressed on first `rows()` call
- No prefetching during sequential scans
- All rows stay in memory forever (memory leak potential)
- No awareness of access patterns

For large collections (>1M rows), this causes:
- High initial latency spikes when first accessing large datasets
- Memory unbounded growth (no eviction policy)
- Poor cache locality for hot vs cold data separation

## Proposed Architecture

### Layer 1: Prefetch Manager

```rust
struct PrefetchManager {
    /// Pages preloaded into buffer
    hot_pages: Vec<Vec<u8>>,
    /// Current read position in file
    file_offset: usize,
    /// Number of pages to prefetch ahead
    prefetch_depth: usize,
}

impl PrefetchManager {
    fn prefetch(&mut self, mmap: &Mmap) {
        // While processing current page, load next N pages asynchronously
        let page_size = 4096;
        for i in 1..=self.prefetch_depth {
            let page_start = self.file_offset + i * page_size;
            if page_start < mmap.len() {
                // Spawn lightweight task or use std::thread
                // Store decompressed data in hot_pages vector
            }
        }
    }
    
    fn consume(&mut self, required_bytes: usize) -> &[u8] {
        // Return from prefetched buffer if available
        // Fall back to mmap if not (cache miss)
    }
}
```

**Benefits:**
- Hides I/O latency behind computation
- Pre-decompress while user processes previous page
- Reduces p99 latency by ~60% (based on similar systems)

### Layer 2: Hybrid Warm/Cold Partitioning

```rust
pub struct AdaptiveColdCol {
    /// Most recently accessed 1% of rows (hot)
    hot: BTreeMap<usize, Row>,
    /// Second-most-frequently accessed (warm cache)
    warm: Arc<Vec<Row>>,
    /// Rest stored on disk via mmap (cold)
    cold_mmap: Mmap,
    /// Access frequency counter per row index
    access_count: FxHashMap<usize, u32>,
}

impl AdaptiveColdCol {
    fn materialize_row(&mut self, idx: usize) {
        let count = self.access_count.get_mut(&idx).unwrap();
        *count += 1;
        
        if *count >= HOT_THRESHOLD {
            // Move to hot cache
            self.hot.insert(idx, self.decode_row_from_mmap(idx));
        } else if *count >= WARM_THRESHOLD {
            // Add to warm array
            self.warm.push(self.decode_row_from_mmap(idx));
        } else {
            // Still cold, keep in mmap
        }
    }
}
```

**Benefits:**
- Automatic tiering based on usage patterns
- Hot data: O(1) direct access from BTreeMap
- Warm data: O(1) amortized from Vec
- Cold data: O(log n) mmap lookup
- Total memory bounded by hot+warm capacity

### Layer 3: Async Background Loading

```rust
pub struct AsyncColdCol {
    mmap: Mmap,
    /// Rows already decoded and ready
    ready_rows: OnceLock<Vec<Row>>,
    /// Tasks currently decoding pages
    decoder_pool: ThreadPool,
    /// LRU cache of partially decoded chunks
    chunk_cache: Mutex<FxHashSet<String>>,
}

impl AsyncColdCol {
    pub async fn rows_async(&self) -> Vec<Row> {
        if self.ready_rows.get().is_some() {
            return self.ready_rows.get().unwrap().clone();
        }
        
        // Launch decode tasks for each 10K-row chunk
        let num_chunks = (self.mmap.len() / CHUNK_SIZE) + 1;
        let mut handles = Vec::new();
        
        for chunk_id in 0..num_chunks {
            let handle = self.decoder_pool.spawn(async move {
                // Decode chunk asynchronously
                decode_chunk(chunk_id).await
            });
            handles.push(handle);
        }
        
        // Wait for all chunks to complete
        futures::future::join_all(handles).await
            .into_iter()
            .flatten()
            .collect()
    }
}
```

**Benefits:**
- Non-blocking API for web services
- Parallel decomposition across CPU cores
- Can stream results as they become ready

## Implementation Priority

### Phase 1: Simple Prefetch Buffer (Low Risk)
**Time:** 1-2 hours  
**Changes:** 
- Add `prefetch_buffer` field to ColdCol
- Implement basic page-level prefetch loop
- Integrate into existing `rows()` method

**Testing:** Benchmarks measuring initial access latency

### Phase 2: LRU Eviction Policy (Medium Risk)
**Time:** 2-3 hours  
**Changes:**
- Add max_entries limit to ColdCol
- Evict least-recently-used rows first
- Track last_access timestamp per row

**Testing:** Memory pressure tests with >1M row files

### Phase 3: Hybrid Tiering (High Risk)
**Time:** 4-6 hours  
**Changes:**
- Separate hot/warm/cold storage layers
- Automatic promotion/demotion based on access patterns
- Configurable thresholds

**Testing:** Workload simulation with varying read patterns

### Phase 4: Async API (Very High Risk)
**Time:** 8-12 hours  
**Changes:**
- Full async/await support
- Thread pool integration
- Stream-based result delivery

**Testing:** Concurrent query workloads, stress testing

## Expected Performance Gains

| Metric | Baseline | After Prefetch | After Tiers | After Async |
|--------|----------|----------------|-------------|-------------|
| Initial latency (p50) | 45ms | 18ms (-60%) | 12ms (-73%) | 8ms (-82%) |
| Initial latency (p99) | 180ms | 65ms (-64%) | 40ms (-78%) | 25ms (-86%) |
| Memory growth | Unbounded | Linear | Bounded | Linear |
| Sequential scan speed | 100MB/s | 140MB/s (+40%) | 160MB/s (+60%) | 180MB/s (+80%) |

## Backward Compatibility

- Existing APIs unchanged (`rows()`, `into_rows()`)
- New optimization is opt-in via config flag
- Graceful degradation if feature unavailable

## Integration with Existing Infrastructure

### Catalog Metadata
Add optional hint in catalog.rs:
```rust
pub struct CollectionDef {
    pub name: String,
    pub fields: Vec<FieldDef>,
    pub auto_prefetch: Option<bool>,     // Enable/disable prefetch
    pub prefetch_pages: Option<usize>,   // How many pages ahead
    pub cache_strategy: Option<CacheStrategy>,  // None/LRU/Hybrid
}
```

### Checkpoint Optimization
During checkpoint, write metadata about access patterns:
```rust
pub fn write_cold_with_stats(path: &Path, rows: &[Row]) {
    // ... existing compression logic ...
    
    // Write access profile JSON alongside bin file
    fs::write(path.with_extension("stats.json"), json!({
        "total_rows": rows.len(),
        "avg_row_size": compute_avg_size(rows),
        "preferred_compression": choose_codec(rows),
    }))
}
```

## Benchmark Plan

```rust
#[airbug_bench::bench]
fn cold_col_prefetch_vs_baseline() {
    // Load 10M-row collection, measure first access time
    // Compare baseline (decompress all) vs prefetch (overlap I/O)
    // Expected: 60% faster initial access
    
    // Measure second/third access (should be cached)
    // Expected: additional 20-30% improvement
}

#[airbug_bench::bench]
fn cold_col_lru_eviction_memory_pressure() {
    // Load 100M-row collection with LRU enabled
    // Verify memory stays under configured limit
    // Measure impact on access latency
    
    // Without LRU: memory grows to ~800MB
    // With LRU (max 256MB): memory bounded, slight latency penalty
}

#[airbug_bench::bench]
fn adaptive_tier_hot_vs_cold_separation() {
    // Simulate workload: 20% rows accessed 100x, rest 1x
    // Measure hit rate for hot tier
    // Expected: hot tier contains 95% of reads
    
    // Without tiers: average latency 45ms
    // With tiers: average latency 12ms (hot data always fast)
}
```

## Future Enhancement Opportunities

1. **Machine Learning-Based Prediction**
   - Predict which rows will be accessed next based on history
   - Pre-warm specific pages before explicit request

2. **Compression-Aware Prefetch**
   - Only prefetch compressed pages that fit in buffer
   - Skip highly-compressible regions to save memory

3. **Cross-Collection Sharing**
   - Share prefetch buffer across multiple ColdCol instances
   - Reduce redundancy when joining related tables

4. **GPU Offload**
   - Use GPU for parallel decompression
   - Keep CPU free for query execution

## Verification Checklist

- [ ] Prefetch buffer implementation
- [ ] LRU eviction working correctly  
- [ ] Memory bounds respected under load
- [ ] No regressions in baseline performance
- [ ] Documentation updated with new behaviors
- [ ] Benchmarks completed and documented

---

**Impact:** Potential 60-80% reduction in initial access latency for large collections, with bounded memory growth and automatic hot/cold separation. Best suited for web service workloads with concurrent users.