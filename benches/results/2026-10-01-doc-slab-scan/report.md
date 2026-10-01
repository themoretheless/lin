# One-pass document slab registration

The docs bulk registration path extracts id, uri, title, layer and wing with one row traversal instead of five BTreeMap lookups. Existing Arc text sharing, missing/nontext defaults, duplicate identity last-writer semantics and fallback behavior unchanged. Other collections unchanged.

Full offline workspace tests pass. Added 64-row reference test compares bulk registration to single-row registration with missing, null, integer, extra and repeated text values; checks both identity maps and all four mirrored document columns. Scoped formatting and diff checks pass.

Six alternating native process pairs and six durable pairs; 24 fresh fixtures per case, one operation. Exact field validation outside timer. Full default Lin behavior and durable fsync preserved. Pinned optimized binaries; tests/builds did not overlap timing. Host load uncontrolled, fixed case order within process. Milliseconds.

## Native inserts

| Pair | Lin 1k baseline | candidate | Lin 10k baseline | candidate |
|---:|---:|---:|---:|---:|
| 1 | 1.025917 | 0.964875 | 11.011416 | 10.582125 |
| 2 | 1.025021 | 0.967270 | 11.331083 | 10.330229 |
| 3 | 1.013917 | 0.949104 | 11.048458 | 10.586354 |
| 4 | 1.009604 | 0.987708 | 11.343105 | 10.397042 |
| 5 | 1.038813 | 0.959041 | 11.410521 | 10.607541 |
| 6 | 1.064625 | 0.959729 | 11.102521 | 10.525583 |

Both sizes faster in 6/6 pairs; median paired decreases 6.17% (1k), 6.12% (10k). SQLite controls have median decreases -2.96% and +0.18%. Native 10k candidate beats SQLite in 4/6 same-process comparisons, DuckDB Appender only 1/6; not a stable peer victory.

| Rows | Candidate Lin median | SQLite same-process median | DuckDB Appender median |
|---|---:|---:|---:|
| 1k | 0.962302 | 0.828156 | 1.224448 |
| 10k | 10.553854 | 10.510906 | 10.361812 |

## Durable inserts

| Pair | Lin 1k baseline | candidate | Lin 10k baseline | candidate |
|---:|---:|---:|---:|---:|
| 1 | 2.188480 | 2.394271 | 20.361000 | 19.851042 |
| 2 | 2.422937 | 2.431167 | 20.302188 | 19.153334 |
| 3 | 2.574625 | 3.878021 | 23.154896 | 25.872959 |
| 4 | 2.624521 | 2.366438 | 20.664667 | 19.468750 |
| 5 | 2.400354 | 2.067833 | 20.660792 | 19.820271 |
| 6 | 2.886917 | 3.317521 | 20.679042 | 19.774041 |

Durable 10k faster 5/6 pairs, median decrease 4.22%; durable 1k faster only 2/6, median decrease -4.87% (slower). Corresponding unchanged SQLite controls +2.11% and -5.47%; large outlier in pair 3. Do not attribute all changes to this code or claim causal significance. Retain the change for consistent native and large durable gains; small durable behavior remains unresolved. All-eight-peer objective remains unproven. Raw observations, baseline source and hashes retained.
