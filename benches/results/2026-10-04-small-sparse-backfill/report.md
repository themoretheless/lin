# Rejected small-batch sparse count backfill

Baseline main 66d3361. Candidate applies a single-pass sparse encoder only when the vector column has fewer than 4096 rows. Preserve decoded-budget/dimension checks and eight-float zero-block skipping, reserve a four-byte count header, write entries, then derive count from encoded-entry byte length divided by eight and patch the header. This avoids both a separate count scan and an increment on every entry. Existing large-batch classifier/encoder and serial large/light-column loop are retained. No new format, approximation, allocation table, dependency, CPU worker or durability tradeoff.

This differs from the rejected 2026-10-01 global single-pass experiment, which used a per-entry increment and affected 10k too; that earlier experiment had an initial 1k gain and a large-case regression. Here the actual loop only changes below 4096. That does not guarantee unchanged timing for the other code paths, and neither assembly nor causal profiling was performed.

## Durable comparisons

Two independent six-pair series using exactly the same pinned baseline/candidate binaries, alternating process order, 24 fresh checked fixtures per case, one operation per sample, no warmup. Builds/tests did not overlap timing. Existing full row/count validation is outside timing. SQLite is an unchanged control. Percentages are medians of paired percentage changes based on process medians, not aggregate-median ratios. Background load/CPU clocks are uncontrolled.

| Series | Lin 1k | SQLite control 1k | Lin 10k | SQLite control 10k |
|---|---:|---:|---:|---:|
| First series | +2.004%, 4/6 | +0.215%, 3/6 | +0.543%, 3/6 | -0.528%, 2/6 |
| Independent repeat | -4.870%, 2/6 | -9.439%, 3/6 | -11.825%, 1/6 | +6.642%, 4/6 |

The first small gain does not reproduce. The repeat has substantial control shifts, so no isolated cause is assigned to the Lin changes, but it provides no evidence to retain the prototype. The larger case also regresses materially despite unchanged loop selection and an improving SQLite control. Reject; do not preserve a first-series win while discarding contradictory repeat results.

## Verification and restoration

All eleven focused WAL integrity tests and full workspace tests passed. New coverage compares single-pass bytes to the existing serial codec over 1001 mixed vectors, forcing buffer growth from a tiny prefix, None/Some(empty), negative zero, NaN payload, infinity and a partial final block. The actual frame payload is checked against the same reference. Direct decoded-budget failures check that a row rejected before emission preserves the previous prefix and that metadata-only overflow is still rejected. Existing scalar-reference parallel frame, checksum/truncation and shared-column budget/no-file-write tests passed.

Production src/persist.rs and the added test were restored byte-for-byte from before-persist.rs. Baseline executable hash matches the retained sparse-classifier candidate; intervening commits contain evidence only. Raw reports, source/binary hashes, test/build logs and exact scripts remain here. Previously confirmed WAL improvements and read matrix are unchanged. Small-write gaps and full named-peer proof remain unresolved; the goal stays active.
