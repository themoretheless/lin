# Unchanged SoA text allocation diagnostic

Nine fresh release processes on macOS ARM64, three per shape. Source and binary hashes are recorded in provenance.json. A System-delegating global allocator counts successful allocations/requested bytes only around 1000 direct Store::row_maps_sync_updated calls. Fixture/setup, row clones, result checks and JSON serialization are outside the counter interval.

| Unchanged title/layer/wing | Allocations per 1000 calls | Requested bytes |
| --- | ---: | ---: |
| Three inline short strings | 3000 | 72000 |
| Three existing Arc strings | 0 | 0 |
| Two inline short, one Arc | 2000 | 48000 |

All three repeats match exactly. The method replaces three SoA Arc slots even when the before/after text is unchanged. Cell::text_shared allocates Arc storage for SmallText and clones an existing Arc for Text. Wing/title access and the original stored row were checked after each interval.

This is an isolated method diagnostic, not an executed DB UPDATE: the only after-row change is ts, and the stored row is deliberately untouched. No timing, RSS, net-live-memory, durability, peer comparison or end-to-end performance qualification is claimed. Production source remained byte-identical after the temporary example was removed. The initial missing InsertBulk.edges fixture compile error is retained separately.

Next hypothesis: retain an existing SoA Arc when its value is unchanged, while still replacing changed text and preserving packed-title invalidation, rebuild fallback, rollback, identity and index behavior. This requires separate regression coverage and a frozen paired benchmark; fewer allocations alone cannot qualify adoption.
