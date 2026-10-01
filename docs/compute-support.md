# Compute support and verification

Verified locally on 2026-10-01, Apple M4 Max, Metal. Enable with Cargo feature
`gpu`, then attach an `Arc<GpuCompute>` to `Db` or use `lin run --gpu`.

## Coverage audit

| Requirement | Implementation | Direct verification |
| --- | --- | --- |
| Hardware device, explicit features/limits, no software adapter | `GpuOptions`, `GpuCompute::new/with_options` | Hardware adapter initialization and resource-limit tests |
| Arbitrary WGSL entry points and resident repeated dispatch | `compile_wgsl`, `dispatch`, `storage_buffer`, `read_buffer` | `arbitrary_wgsl_resident_dispatch_and_errors`, `examples/compute.rs` |
| Storage/uniform resources, textures, multiple groups and 3D dispatch | `gpu::native`, native device/queue, reflected layouts | `storage_texture_uniform_and_multiple_groups`, arbitrary dispatch test |
| Validation/allocation errors, bad shader/entry/layout/index/dispatch, subsequent usable device | `checked`, checked `bind_group_layout`, dispatch/readback validation | Arbitrary shader error test and limited-resource tests |
| Exact scalar comparisons and boolean combinations | `compare.wgsl`, `boolean.wgsl` | Numeric boundary/special-value and mixed predicate tests |
| UTF-8 text equality, substring, words, case folding | `text.wgsl` | Unicode/null/empty/mixed text tests |
| Regex boolean matching | `regex.wgsl`, serialized Thompson NFA | Library regex corpus and executor/cursor tests, including assertions and invalid patterns |
| Group count and source-order binary64 sum | `count.wgsl`, `sum.wgsl` | Counts, grouped/batch/snapshot sums, bitwise finite-value and exceptional-value tests |
| ID and nonunique joins | `join_id.wgsl`, `join.wgsl` | Duplicate/order/absent-key, tiled, batch, multiple-join cursor tests |
| Stable scalar sorting and exact i64 ranking | `sort.wgsl` | All cell kinds, directions, ties, odd lengths and integers beyond f64 precision |
| Lexical weights, cosine, vector ranking and hybrid RRF | `text/weights/cosine/sum/sort.wgsl` | Lexical/Unicode/repeated terms, resident vectors, bitwise RRF, identity ties and dispatch counters |
| Graph BFS and match DFS | `walk.wgsl`, `match.wgsl` | Reverse/cycles/duplicates/depth/order/300-result boundaries and unresolved nodes |
| Db, batches, ReadDb and cursors | Executor and cursor GPU branches | Operator integration tests, mutation/rollback/snapshot cache tests, stage-order and mixed-collection tests |
| Concurrent async queries and streams | Existing blocking offload with shared hardware backend | Eight concurrent clients; query and stream results plus real dispatch counters |
| Fixture and durable CLI, explicit feature opt-in | `run --gpu`, `--data` | `compute_cli` compares CPU/GPU after durable reopen; feature-disabled test checks explicit failure |
| Portable baseline shader validation and reproducible build checks | `tests/shaders.rs`, `.github/workflows/compute.yml` | All 14 WGSL modules validate without optional capabilities; local API/CLI/example build succeeds |

Verification command, 149 passing tests including 30 hardware checks:

```sh
cargo test --release --features gpu --lib --test gpu --test shaders \
  --test compute_cli --test exec --test query --test embed -- --include-ignored
cargo check --locked --features gpu --lib --bin lin --examples
cargo test --release --no-default-features --test compute_cli
```

The counts describe this run, not the entire repository test suite. Hardware tests
are ignored by default so a machine without a physical GPU can build and run
the ordinary suite. The compute workflow compiles on Linux/macOS/Windows and
validates WGSL without an adapter; its remote execution has not been run here.
Actual Vulkan/DX12 execution is not verified by the Metal run.

## Execution boundaries

The native API exposes wgpu resource creation and command encoding for advanced
compute use. Direct native calls retain wgpu's validation/synchronization contract;
the convenience API converts scoped validation and allocation failures to Lin errors.
Do not nest `checked` calls.

Query kernels run on hardware. Parsing, storage/WAL, embedding generation, FTS
postings, Unicode assertion metadata, dictionaries/key packing, row projection,
skip/take/union and result assembly remain host operations. Query stages are not
fused into a single fully resident GPU plan. Graph traversal and source-order sum
use sequential GPU work where required for existing order semantics; a speedup
over CPU is not promised.

Cosine uses f32 while CPU uses f64, so very close scores and the 0.01 threshold
can differ. Numeric filters and sorting preserve existing scalar comparison rules;
sum/RRF implement binary64 with u32 limbs, without requiring native shader f64.
Sum canonicalizes NaN payloads. Regex matching uses GPU state transitions while
CPU prepares Rust-compatible Unicode/look-assertion metadata.

Device buffer/dispatch limits are enforced. Oversized graph/regex/sort state
returns an error; it does not silently switch matching or traversal to CPU.
Vector cache contents are invalidated on corpus identity/order/dimension changes,
including direct Store mutations. GPU resources are not persisted in WAL/snapshots.
