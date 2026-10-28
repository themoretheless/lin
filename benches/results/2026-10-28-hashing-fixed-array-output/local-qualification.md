# Functional qualification

12focused embedding tests and450release library/integration tests passed,2ignored. Original dense-bit oracle and threshold/dimension/large-normalization/reset/held-output regression retained. Missing-coordinate mutant failed at runtime (0passed/1failed),every source hash restored. Exact mutant source/log and all test logs retained.

Only output materialization region differs from rejected2c8515692; normalization/features/tests and separate reset are byte-identical outside it. Current embed.rs contains no unsafe or MaybeUninit. Standalone new/get_mut/array-to-slice Arc operations compile/run on Rust1.85, with identical data address across coercion. This is not complete crate MSRV qualification, allocation-count proof or performance evidence.

No speedup or adoption claimed. Same baseline/controls/frozen gates in protocol.md; complete source/engine and original eight-peer qualification still required.
