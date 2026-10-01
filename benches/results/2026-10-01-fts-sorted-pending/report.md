# Sorted FTS pending edits: correctness fix

The pre-existing merge helpers require sorted, unique inputs. Pending additions/removals were appended without sorting; cancellation used swap_remove, which also broke order. Deleting positions 8 then 2 from a 0..9 posting list left position 2 in matches. The new regression failed on the original two-pass implementation as well as the proposed fused implementation.

Pending insertion now uses binary_search plus ordered insert, deduplicating repeated edits. Cancellation uses binary_search and ordered remove. The fused-merge experiment was removed; performance comparisons must use a correct baseline.

Validation: workspace tests passed after the production fix. Two targeted regression tests passed on the final source: out-of-order edits with cancellations; descending removals past the fold threshold and 3000 deterministic random edits checked against a BTreeSet oracle at each step, plus fold/codec roundtrip.

## Current native deletion

One process, 12 fresh-fixture observations per case. Readback and affected counts are checked outside timing. This is verification, not a paired speedup claim.

| Rows | Lin µs | SQLite µs |
|---|---:|---:|
| 1k | 9.104 | 4.729 |
| 10k | 13.166 | 10.396 |
| 100k | 21.395 | 16.521 |

No universal native-delete win is established. Existing historical FTS mutation profiles/timings did not prove arbitrary-order pending correctness. The fix preserves the base-only persisted format; it does not repair already-corrupted derived posting blobs automatically.

Scoped rustfmt and git diff --check passed. Concurrent workspace work added an optional GPU module; a transient cargo fmt --all failure reported its then-missing file. Those changes were preserved; the checks here use the default feature set and do not establish GPU validation.
