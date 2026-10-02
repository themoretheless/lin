# FTS sorted posting union

Retained optimization: candidate_idxs consumes already sorted unique live
posting lists directly. The first matching list moves into the result; later
lists merge linearly instead of inserting every row into a BTreeSet. Pending
additions/deletions still pass through the existing matches merge. Returned
positions remain sorted and unique.

Six alternating independent process pairs; 32 samples per case, warmup 50ms,
sample target 5ms, at most 100 operations per sample. Fixture setup excluded.
Same benchmark binary sources except src/fts.rs; no builds/tests overlap timing.
Host load is uncontrolled. Process medians aggregated before comparison.

| Query on 10k docs | Baseline µs | Candidate µs | Paired median gain | Winning pairs |
|---|---:|---:|---:|---:|
| Frequent term | 184.383 | 162.038 | 12.14% | 6/6 |
| Selective term | 0.731 | 0.696 | 6.03% | 5/6 |
| Missing term | 0.276 | 0.265 | 4.82% | 5/6 |

The strongest evidence is the frequent-term case. Submicrosecond differences
are sensitive to noise. This is an internal before/after comparison; peer
engine speed and durable insertion are not inferred from it. Raw samples,
source copies and SHA256 hashes are retained. A scan-reference regression test
covers multiple/duplicate/Unicode/missing terms with pending edits.

Validation: cargo test --offline --workspace passed, including the new scan-reference
regression. Final cargo bench --offline --bench compare --no-run passed. Scoped
rustfmt, Python benchmark contract tests (7), and git diff --check passed.
