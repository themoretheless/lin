# NEON sparse WAL serialization

AArch64 uses an exact raw-bit presence mask for each complete eight-float block
of a selected sparse vector column. Integer NEON operations distinguish positive
zero from every other bit pattern, including negative zero and NaN payloads.
Trailing-zero traversal writes the original ascending index/value byte stream.
Incomplete blocks use scalar masks.

The call site checks NEON availability before entering target-feature functions.
Other architectures or unavailable NEON use the retained original scalar writer.
The unsafe load helper asserts eight elements and loads only those eight values;
unaligned subslices are covered by a finite independent scalar-byte oracle.

Classifier thresholds, dense codec, dimension/budget/error checks, row order,
framing, CRC and sync are unchanged. No persistent cache or allocation is added.
This is distinct from earlier rejected scalar masks and NEON-only classification.

[Full-engine evidence](../benches/results/2026-10-26-neon-sparse-writer-engine/report.md)
supports a limited durable1k improvement on the measured AArch64 fixture.
It does not close the remaining SQLite1k gap, prove a robust additional10k gain,
or establish all-eight-peer superiority. CPU-only and density-crossover reports
are linked there; non-AArch64 runtime and physical power-cut qualification were
not performed for this change.
