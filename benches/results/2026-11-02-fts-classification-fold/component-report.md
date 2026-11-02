# FTS three-state classification fold: local component discovery

Candidate folds high-bit presence and uppercase-ASCII presence into two bits in one non-short-circuit byte pass.0 borrows lowercase ASCII,2 copies and lowers ASCII,1/3 retain the original Unicode character mapping. No unsafe code, new dependency, architecture-specific intrinsic or feature change. Unlike the earlier rejected any()-based combined scan, it does not rescan ASCII after a positive classification.

Both complete extracted functions are in one rustc -O executable; base/control use the same original function. Two balanced six-round series,7 datasets,252 fresh processes. Correctness assertions precede/follow timing; every process also checks ASCII and marker placement across lengths0..257. All observations retained. Compiler/host/source/binary fingerprints in status.json. Longer fixed loops differ from the earlier rejected prototype; do not pool their timings.

| Series | Dataset | Gain % | AA % | Positive pairs |
|---|---|---:|---:|---:|
| 1 | short_lower | +64.568 | 2.244 | 6/6 |
| 1 | body_lower | +48.904 | 5.290 | 6/6 |
| 1 | long_lower | +91.464 | 3.404 | 6/6 |
| 1 | short_upper | +2.021 | 21.669 | 3/6 |
| 1 | late_upper | +85.303 | 1.456 | 6/6 |
| 1 | unicode_lower | +1.804 | 2.762 | 6/6 |
| 1 | unicode_upper | -0.219 | 1.075 | 2/6 |
| 2 | short_lower | +64.374 | 4.636 | 6/6 |
| 2 | body_lower | +48.285 | 4.302 | 6/6 |
| 2 | long_lower | +91.240 | 3.058 | 6/6 |
| 2 | short_upper | -11.187 | 19.316 | 2/6 |
| 2 | late_upper | +84.975 | 2.818 | 6/6 |
| 2 | unicode_lower | -0.319 | 3.106 | 3/6 |
| 2 | unicode_upper | +0.023 | 0.878 | 3/6 |

Lowercase ASCII and late-uppercase results justify testing integration; short-uppercase and Unicode results remain mixed/noisy. This is discovery, not a predeclared all-DB acceptance result. No database, DuckDB or SQLite speedup is inferred. Runtime remained unchanged during this standalone diagnostic; applying the candidate afterward is a separate unqualified source change. The live full Linux run451fd7a25 still measures60e64559e and excludes this fold. Default/full feature correctness, bit-exact embedding/FTS regression, actual native insertion, full source controls and all peers remain binding before adoption.
