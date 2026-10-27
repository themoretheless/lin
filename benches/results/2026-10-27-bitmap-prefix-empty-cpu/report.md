# PostingList emptiness CPU controls

Six processes,14shapes each,24rotated A/A/B samples per shape/role. Baseline/control call the unchanged count()==0; candidate calls the proposed is_empty.84case records,6048timed batches,14819328kernel calls, exact membership truth checked every batch. One binary and the same list object per shape/role; no database or peer latency claim. Includes first-word, last-word and fully empty bitmaps at1024/16384/131072/1048576bits plus empty/nonempty Standard vectors. Raw ns/sample and absolute/relative process summaries retained.

Bounded first-word probe: large sparse/empty fallbacks near original count;1024-bit fallback costs about0.25ns extra, retained as a limitation.

The probe temporarily compiles inside the private index module with a narrow test-only library bridge; both source files are restored before measurement processes run. An initial external-module compile attempt failed because index is private (retained for the naive probe). No production API is exposed or changed by the probe. Large percentages on sub-nanosecond absolute differences in Standard controls must not be called whole-engine regressions. Fresh-fixture qualification is separate.
