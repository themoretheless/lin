# BTreeMap Row pooling and clone_from: allocation audit

Decision: reject the hypothesis that the existing empty-Row pool or standard BTreeMap::clone_from removes the measured full-row clone allocations. Do not replace mutation clones with these mechanisms on the strength of their names or comments. This audit does not reject all possible reuse designs; a different design must preserve the public empty-acquire contract, values, ownership, resource limits and full lifecycle, then qualify CPU/engine latency separately.

Production Rust source remains byte-identical to04749994. Three fresh local processes, System allocator counters only, nine-field synthetic docs-shaped Row including an Arc-backed768-element embedding. Four primitive operations, three1000-operation warm repeats per warm case per process, plus one cold pool operation per process. 27000 warm operations and three cold operations in total. Source/rustc/probe/binary hashes and raw counters saved. No CPU time, RSS, Linux or full-engine performance claim.

| Primitive | Allocations per operation | Requested bytes | Frees | Deallocated bytes |
|---|---:|---:|---:|---:|
| clone + drop | 10 | 582 | 10 | 582 |
| clone_from with held initialized destination | 10 | 582 | 10 | 582 |
| warm pool acquire/fill/release | 10 | 582 | 10 | 582 |
| cold TLS pool acquire/fill/release | 11 | 2118 | 10 | 582 |

All three processes and all warm repeats matched exactly. Cold TLS adds one1536-byte requested allocation retained by the empty map cache until thread exit. This is requested allocator memory in this scope, not resident memory. Source creation, Store construction/global pool initialization, target clone_from initialization and reporting are excluded. For clone_from, the held target is not dropped inside each operation: replacing its former contents still frees ten allocations, and a current target remains live across calls. clone_drop and pool operations include release of row allocations. Cold TLS cleanup at thread exit is outside counting; no full-lifecycle saving is claimed.

The current release implementation calls row.clear(), which frees the BTreeMap nodes and all String keys. Its comment about keeping capacity is misleading for this container. The pool caches empty map descriptors, not reusable tree nodes. On the inspected local standard-library implementation, BTreeMap Clone does not override clone_from, so the trait default constructs a new clone and replaces the old destination. Runtime counts independently confirm no allocation saving for this fixture.

Probe uses the public Store::acquire_pooled_row and Store::release_pooled_row methods. Full values are compared on independent direct/pool/clone_from validation operations outside allocation counting; acquired rows are checked empty. No prototype allocator, production function or public type is modified. The first example build tried to access the private store module and failed at visibility checks; build-first-failure.log preserves that error. It was corrected to use the existing public Store methods, with no export added. The temporary example is removed after execution.

There are many internal acquire_row calls but no internal release_row caller beyond the public Store wrapper. This source observation is not a complete downstream usage audit. It does not justify removing the public pool API or claim a material speedup from bypassing TLS. No further full-engine run of these unchanged allocation hypotheses is warranted before a different mechanism is demonstrated.
