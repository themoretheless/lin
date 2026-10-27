# Experimental COW Row: owning adapters

This adds only explicit consuming adapters to the archived COW candidate, with no production change or API migration approval. Production Row remains the exact BTreeMap alias. The Linux40502052753 full-engine qualification uses the previous frozen source, not this modified candidate; its performance evidence cannot automatically qualify these adapters or a future integrated release.

Added into_keys(self), into_values(self), and From<Row> for BTreeMap<String,Cell>. All delegate owning map extraction to Arc::unwrap_or_clone. Unique container ownership takes the map; shared ownership clones it. Allocation/latency of these methods is not measured here. Iterator return types are exactly the standard btree_map::IntoKeys/IntoValues, including DoubleEndedIterator, ExactSizeIterator and FusedIterator behavior. No Row fields, serialization derives or existing execution paths are changed relative to the frozen candidate.

Six runtime downstream-style tests pass on both production baseline and modified candidate: unique/shared map conversion isolation; sorted consuming keys; ordered consuming values; empty conversions; value release while consuming/early-dropping keys; value release while consuming values and dropping owned maps. Arc payload counts check that values held by returned Cells remain live and that all references are eventually released. This is bounded ownership/compatibility coverage, not exhaustive public API qualification.

The complete default library/integration suite with existing frozen COW oracles and the six new tests passed:416 passed,2 existing ignored. All four temporarily replaced core files were restored and temporary test/example files deleted. Hashes, compiler/test logs and independent verification are retained.

A separate compile probe still demonstrates the inherent API break: `let row: Row = BTreeMap::<String,Cell>::new()` compiles on baseline, fails with E0308 on the modified COW type. Explicit conversions and consuming iterator gaps are addressed; exact implicit type identity is not. The existing human API preference question remains pending. No answer or migration authorization is inferred from automatic continuations.

Next decisions depend on the broader COW measurement and the API choice. Any integrated candidate still requires its own source/binary qualification, native/durable/read/memory controls and named-peer comparisons. This archive is neither an accepted performance improvement nor evidence of all-eight-peer superiority.
