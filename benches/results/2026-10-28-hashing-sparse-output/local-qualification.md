# Functional qualification

12 focused embed tests passed, including the original independent dense feature-hash bit oracle and a new occupancy/dimension/large-norm/held-output regression.450release library/integration tests passed,2ignored. The missing-coordinate mutant failed at runtime (0passed/1failed), and every candidate source hash was restored. Raw logs and exact mutant source preserved.

Standalone allocation/initialization/assume_init operations compile and run on Rust1.85, below project MSRV1.88; this is not complete crate/toolchain qualification. Safety argument: fill initializes every element to valid f32 zero before any conversion; subsequent writes use initialized f32 coordinates. No raw pointer, uninitialized read or partially published output is introduced.

No performance measurement or acceptance yet. Hypothesis and unchanged timing gates are in protocol.md; complete engine/eight-peer objective remains open.
