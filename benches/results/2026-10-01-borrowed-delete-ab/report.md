# Borrowed moved-row deletion: retained

Deletion previously cloned the complete moved Row to edit its derived structures and then re-registered already transferred column values. The candidate reads the moved row in place while updating disjoint scalar/FTS fields, retains only shared identity keys for ID/URI/SPO maps, and uses the existing swap-pop of parallel columns. The identity maps keep remove/register semantics, including duplicate identities. Scalar insertion keeps the first-error boundary. Public Row and persistence formats remain unchanged.

The fast path checks all mirrored column lengths once before deletion. Nonparallel columns retain the old clone/register/rebuild path. No row is temporarily removed or replaced with an empty placeholder; this differs from the previous rejected mem::take moved-row experiment. Timing covers CAS, undo and full indexes/FTS as before; no feature was disabled.

Baseline and candidate were built from a frozen snapshot. Only src/store.rs differed, including tests. Default workspace tests passed for the initial candidate; final targeted tests passed after preserving the scalar first-error boundary. The integrated final workspace test result is in tests-live.log and validation.json.

The two new tests compare repeated multi-position deletions against rebuilt rows, maps, every mirrored column, scalar forward/reverse maps and exact FTS positions in docs, orders, users, facts and a custom collection. Duplicate/dead/out-of-range requested positions are included. A separate test exercises the nonparallel-column fallback.

## Native deletion

Three independent prebuilt process pairs with alternating order, 24 fresh samples per case, one timed operation each. Setup/destruction and existing affected/count/readback checks are outside timing. Our compilation/tests completed before benchmarking. Unrelated host activity remains uncontrolled; background process inventory is retained. In several cases SQLite controls improved too, so full timing reductions cannot be attributed solely to the code change; no statistical significance is claimed.

| Pair | Rows | Baseline Lin µs | Borrowed Lin µs | Reduction | Baseline SQLite µs | Candidate-process SQLite µs |
|---|---|---:|---:|---:|---:|---:|
| 1 | 1k | 8.979 | 5.042 | 43.9% | 5.792 | 2.812 |
| 1 | 10k | 13.854 | 12.209 | 11.9% | 8.208 | 7.521 |
| 1 | 100k | 18.625 | 19.812 | -6.4% | 16.583 | 16.729 |
| 2 | 1k | 12.209 | 5.667 | 53.6% | 7.438 | 3.792 |
| 2 | 10k | 15.416 | 13.688 | 11.2% | 13.896 | 11.250 |
| 2 | 100k | 22.145 | 21.312 | 3.8% | 16.167 | 18.771 |
| 3 | 1k | 12.563 | 7.979 | 36.5% | 4.021 | 3.333 |
| 3 | 10k | 16.396 | 13.959 | 14.9% | 12.541 | 10.688 |
| 3 | 100k | 23.459 | 22.146 | 5.6% | 15.521 | 18.500 |

## Decision

- 1k: 3/3 candidate wins, median paired time reduction 43.9%.
- 10k: 3/3 candidate wins, median paired time reduction 11.9%.
- 100k: 2/3 candidate wins, median paired time reduction 3.8%.

Retained as a reduction in row-cloning and redundant column registration with positive paired observations. The 100k result is mixed; no universal deletion acceleration is proved. Lin still loses SQLite in all nine candidate-process size comparisons, so deletion and the full eight-peer goal remain incomplete. No RSS/allocated-byte reduction was measured.

Source variants, common/binary hashes, six successful raw measured runs, build/test logs and a source archive are retained. Build-only Cargo invocations intentionally matched no cases and exited 1 after successful compilation. Live integration replaces only the deletion section and adds its tests, preserving other concurrent changes.
