# Corrected UPDATE patch integration: local correctness passed

This candidate combines the Linux-qualified terminal checkpoint correction79a4cb837 with the UPDATE patch clone-elision implementationed3098197. Runtime changes are confined to exec.rs relative to the corrected checkpoint base; other runtime source files and dependencies remain unchanged. Explicit hash guards remain distinct from hashes derived from body, and schema rollback retains the earlier row/append journal.

The combined source passed all458 default release tests/two ignored on macOS ARM64, including30 schema rollback scenarios and the explicit/derived hash guard test. Source/test hashes and the complete log are retained. No timing result or Linux acceptance for this combined source is established yet.

The earlier isolated UPDATE component completed504processes/4032observations and passed all six native targets with32.295–46.902% gains, but failed the DuckDB filter_eq control. Its overall gate remains rejected; no observation is removed or threshold waived. Those gains must not be multiplied with earlier COW/WAL gains or transferred to this new source.

The next complete comparison needs an independently corrected BTreeMap reference preserving the exact public Row=BTreeMap contract. The same schema defects have now been reproduced on original6812cc5ca. Correctness parity is required before a full-source matrix, followed by current full-value peer comparisons. The eight-peer objective remains open.

The latest 458-test result is isolated-full-release.log from a dedicated target directory. A prior shared-target run omitted six COW tests (452 observed); it is preserved as shared-target-incomplete-local.log and is not accepted. CI checks both the count and six named COW tests. Linux qualification remains pending.
