# Full COW comparison with an invariant external-peer executable

This is a new protocol revision, not a replay or waiver of runs41353987944/41363558777/41366596325. Those failed acceptance decisions remain recorded. The eight-peer objective remains unchanged and unachieved.

Motivation: the original linked-harness comparison exposed a persistent33–35% DuckDB GROUP BY latency difference between BTreeMap/COW executables despite identical DuckDB SQL/dependency source. Static audits of exact ELF files found equal instructions/lengths after masking address operands; constructor/main allocation constants also match. No physical cause was established. An external engine compiled into a different Lin executable is not an invariant reference.

Predeclared assignment:

* Lin baseline/control use exact e3136c0a47ef9081a351571b7f65160a80a1c805.
* Lin candidate uses exact625bc7180b37027d661afb87009464617c34de36.
* All SQLite, SQLite DELETE/WAL and DuckDB labels use a separate immutable copy of the baseline-built executable. Its hash must equal the baseline hash, differ from the candidate hash, and retain the same frozen harness. The external executable is fixed by source identity before any measurement.

No SQL, callback, fixture, timer, row values, schema, Full/Normal contract, output lifetime, oracle, quota, allocation environment or peer dependency version changes. The source/harness pair is unchanged from the corrected balanced matrix. The rejected FTS candidate and package-codegen override are not included. The current checkout is overlaid serially for builds and restored byte-for-byte afterwards.

All67 original cases remain:29 Lin and38 external.12 rounds, two six-round series, three roles,2412 fresh processes/19296 observations. The corrected scheduler independently verifies positions, precedence and adjacency for each case. Original five target cases (ten windows) require6/6 positive pairs and median gain above median absolute A/A. All62 controls retain gain >= -max(2%, median absolute A/A), including the external same-binary null controls. External noise failures are retained and can fail the experiment. No acceptance band is widened.

Both actual frozen Linux correctness artifacts are independently audited (BTree441/2 with six compiled negatives, COW458/2 with five compiled negatives and named COW markers); identical dependency, derive and peer-worker build inputs are verified.134 fresh smoke processes exercise every source label/case assignment before timing. A synthetic complete matrix must pass; old order, source/hash/command/smoke/RSS/NaN/summary corruptions and losing target/control gates must fail appropriately. New negative controls substitute a candidate external hash/source/command/role and must be rejected.

The independent raw verifier binds every measured/smoke process to its assigned executable. Exactly1368 external measured assignments must use the same frozen peer hash. Lin/peer ratios compare candidate Lin to this invariant reference. RSS remains whole-process (fixture/oracle/reopen included), not isolated operation memory. Numerical peer superiority and source acceptance are reported separately; no eight-peer completion or production adoption follows from a green source gate alone.

Linux CI: ubuntu24.04 x86_64, Rust1.96.0, jobs2, one job-owned serial Cargo target, no profile override. All frozen inputs are checked before execution. Upload full raw results on failure and separate exact executable artifacts. No unchanged acceptance retry.
