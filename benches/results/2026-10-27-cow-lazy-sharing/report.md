# Lazy sharing COW Row: functional/allocation prototype, not a qualified acceleration

Hypothesis: remove the unconditional Arc allocation paid by fresh rows in the rejected COW source, while retaining shallow clones for returned UPDATE rows. This addresses a plausible mechanism behind its Full-insert1k/equality-COUNT regressions, but does not prove that Arc allocation caused either regression or that the new representation fixes engine latency. Production source04749994 and exact public BTreeMap alias are unchanged; no API migration approval is inferred.

The experimental Row owns either an inline BTreeMap or a shared Arc<BTreeMap>. Fresh/default/deserialized/collected rows start inline. The in-place UPDATE path explicitly promotes the touched live row before cloning its returned result; other seeded rows remain inline. Clone of an inline row deep-copies the map, Clone of a shared row shares the container. DerefMut uses Arc::make_mut only for shared storage. Owning adapters from229a145e are retained; private mode does not affect Eq/Hash, Debug or transparent map serialization. From<Row> consumes the map or unwraps/clones the Arc. Pool release uses this extraction rather than assuming the tuple always contains Arc.

419 default library/integration tests pass,2 existing ignored. These include the frozen COW state/bitmap oracles, six owning API/lifetime tests and three new tests: cross-representation Eq/Hash/JSON/MessagePack/Debug and held-result isolation (including signed zero/NaN hash equality); a1024-step deterministic map model with promotions and held clones; actual public UPDATE promoting only the touched row and preserving earlier output. A deliberate Eq mutant that treats inline/shared values as unequal is rejected by a runtime assertion, not a compiler error. All source and temporary test/example files are restored after execution.

## Allocation counter evidence

Three implementations ×three fresh processes ×three1000-operation repeats ×six primitives:9 successful processes,162000 operations. System allocator wrapper counts allocation calls/requested bytes and deallocation calls/bytes; every successful process/repeat matches. Fixture/Db creation and eventual Db teardown are excluded. Primitive returned rows/clones are dropped inside counting. This is not CPU timing, process RSS or complete engine memory-lifecycle qualification.

| Primitive | Main allocs /bytes | Frozen COW allocs /bytes | Lazy COW allocs /bytes |
|---|---:|---:|---:|
| Empty Row::new/drop | 0 /0 | 1 /40 | 0 /0 |
| One-cell count-shaped construct/drop | 3 /593 | 4 /633 | 3 /593 |
| Nine-cell docs-shaped construct/drop | 11 /1014 | 12 /1054 | 11 /1014 |
| Unpromoted row clone/drop | 10 /582 | 0 /0 | 10 /582 |
| Returned actual UPDATE row clone/drop | 10 /582 | 0 /0 | 0 /0 |
| Returned row clone, mutate/detach,drop | 10 /582 | 11 /622 | 11 /622 |

Frees and deallocated bytes equal allocations/requested bytes inside every measured primitive. This does not imply zero retained fixture/Db memory. The returned UPDATE fixture executes a real public CAS update before counting; detached mutation leaves that result unchanged. All full-value checks are outside counters. Dense768-element embedding payloads remain Arc-backed and are not deep-copied by these row clones.

Measured size_of<Row>:main24 bytes,frozen COW8,lazy COW32; underlying BTreeMap24 on this local toolchain. Removing the Arc allocation trades for a larger inline descriptor and restores map-copy cost for unpromoted reads/snapshots. The existing row_byte_size helper uses size_of<Row>, so representation changes its estimate and can shift quota boundaries; no guard is removed/weakened, but exhaustive threshold/resource compatibility is unqualified. An unpromoted patch clone is also a map clone again, which may offset native UPDATE gains. These costs and all failed controls must remain in future qualification.

First allocation attempt stopped before collecting counters:its actual UPDATE fixture omitted layer/ts and had7 fields while asserting the benchmark's9-field shape. The fixture was corrected to include both fields before the nine successful counter processes. The initial probe source/status/empty output are preserved as failed setup evidence, not accepted samples. Subsequent stderr logs are retained.

## Required next evidence

Exact implicit BTreeMap assignment remains incompatible with a wrapper type. This is an experimental future API candidate with the human preference question still pending, not a shipping change. The successful frozen COW narrow result and failed expanded result cannot be attached to this modified source. Run fresh uninstrumented full-engine qualification against production main with the same UPDATE targets,all native/Full-insert/COUNT and unchanged-peer controls,then broaden to durable mutations,materialization/memory/platform and all eight peers. Preserve every loss. Linux main-path durable phase diagnostic40510531528 is a separate source/run and cannot qualify this Row design. Full objective remains active and incomplete.
