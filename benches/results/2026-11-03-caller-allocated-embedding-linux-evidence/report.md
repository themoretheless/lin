# Caller-allocated embedding outputs: warm win, lifecycle rejection

Run https://github.com/themoretheless/lin/actions/runs/41637335410 at 74fd999ca58d66a70f10ee1a7d147356195ac8a6. Independent audit verified 216 processes, 6156 operation samples, 108 lifecycle wall and 108 RSS observations; runtime/harness/helper source closure, actual compilation, both exact ELF binaries, every result/digest and numerical window. Linux workspace 415 passed / zero failed / 3 ignored. All downloaded members archived and reread against SHA-256 manifest. Numerical rejection, not infrastructure failure.

| Native rows | Series | Median gain % | A/A % | Positive pairs | Passed |
|---:|---:|---:|---:|---:|---|
| 1000 | 1 | 22.203103 | 1.694051 | 6/6 | True |
| 1000 | 2 | 20.542465 | 2.886990 | 6/6 | True |
| 10000 | 1 | 18.955635 | 0.511809 | 6/6 | True |
| 10000 | 2 | 19.927369 | 1.001120 | 6/6 | True |
| 100000 | 1 | 26.413013 | 3.288917 | 6/6 | True |
| 100000 | 2 | 27.073899 | 2.288973 | 6/6 | True |

All six native target windows and all twelve prepared read-control windows passed. All six RSS controls passed. Four lifecycle wall windows failed:

| Rows | Series | Median gain % | A/A % | Allowed loss % |
|---:|---:|---:|---:|---:|
| 1000 | 1 | -2.309209 | 2.127554 | 2.127554 |
| 1000 | 2 | -5.724668 | 2.724936 | 2.724936 |
| 100000 | 1 | -3.104066 | 2.333184 | 2.333184 |
| 100000 | 2 | -3.115001 | 1.407496 | 2.000000 |

The warm operation improvement does not waive fresh whole-process regressions. Lifecycle time includes initialization, statement preparation, one INSERT, post-timer readback, result serialization and teardown. These results do not isolate which phase caused the lifecycle loss. Moving Arc allocation to the caller is promising for the hot operation, but this exact implementation is rejected under the frozen all-window AND rule.

Restore src/embed.rs byte-for-byte to b9f10bc776b05eb5235237e9a900c2ef6aaed1fd. Prototype source is preserved at c022e34178a459bf0c9efb6a51f4423947b35d92. No unchanged numerical retry, main merge, FULL WAL or fresh peer victory. Any follow-up must change the implementation and investigate full-lifecycle costs rather than reclassifying controls. Full eight-peer objective remains unfinished.
