# Rejected FTS append ASCII whitespace split

Baseline main 3478392. Candidate replaces split_whitespace in append_row_with_scratch with split_ascii_whitespace when lowered text is ASCII and contains no vertical tab. Unicode/vertical-tab text keeps the original splitter. Existing lowercase transformation, fields, token order, posting insertion/uniqueness, pending mutations, query tokenization and persisted layout remain unchanged. No allocation table, dependency or CPU worker was added.

## Paired insertion benchmarks

Six alternating baseline/candidate process pairs for native and durable fixtures, 24 fresh checked samples per case, one operation/sample, no warmup. Exact existing fixture validation runs outside timing. Pinned binaries; builds/tests did not overlap timing. SQLite is an unchanged control. Percentages are median paired improvements from per-process medians, not aggregate-median ratios. CPU frequency/background load are uncontrolled.

| Mode | Lin 1k | SQLite control 1k | Lin 10k | SQLite control 10k |
|---|---:|---:|---:|---:|
| Native | -0.374%, 3/6 | +0.642%, 4/6 | -1.683%, 0/6 | +0.356%, 4/6 |
| Durable | +0.230%, 3/6 | +0.981%, 5/6 | -0.396%, 2/6 | +0.076%, 3/6 |

Reject: native 10k regresses in all six pairs while the unchanged SQLite control is slightly positive. Other Lin cases are mixed or slightly negative; no consistent gain is demonstrated. The embedding ASCII-split result cannot be generalized to this posting-update loop. No assembler/profile attribution is claimed and no repeat was run: the six native-10k regressions were sufficient to reject this candidate.

## Validation and restoration

Eight focused FTS tests passed. New coverage compares the complete posting map against the original Unicode whitespace splitter over all 128 ASCII symbols, ten Unicode whitespace characters, Greek/other Unicode, repeated words across title/body, empty text and vertical tab, preserving the existing lowercase helper semantics. Pending adds/dels remain empty for the fresh slab. Existing pending-edit, recycled-tail and reference tests passed. Full workspace was not rerun because production and the new test were restored byte-for-byte from before-fts.rs.

Baseline executable SHA256 matches the retained sparse-classifier candidate metadata. Source/binary hashes, raw outputs, invocation scripts and build/test logs are retained. Prior WAL optimizations and local durable-10k SQLite lead remain unchanged. Native/durable 1k and the full named-peer objective remain unresolved.
