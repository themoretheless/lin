# Adaptive FTS reserve — CPU/capacity qualification

2026-10-24 baseline 24ad364. Follow-up to unconditional postings reserve; production source unchanged. Source-derived tokenization, lowercase, implicit snippet and add_posting functions retained in reference.rs; standalone complete-map reference equality before timing. No independent tokenizer oracle, whole-engine or live-peer qualification.

Variants: 0 original; 1 unconditional min(rows,4096) control; 2 adaptive. Adaptive processes first 32 rows normally. At row offset 32, only for >=128 rows and >=24 distinct observed terms, it reserves to min(rows,4096,8*observed_terms). This bounded extrapolation is an experimental heuristic, not an established cardinality estimator. The per-row branch, initial prefix, reserve allocations and map destruction are included in CPU timing. No pending-edit path is modeled.

Two independent series × six processes, 24 rotating samples per variant/case/process. Real default document rows, repeated two-term corpus, Unicode/snippet corpus and a diverse-prefix/repeated-tail corpus. Fixtures and validation outside timing. Source/binary hashes, compiler version, source-derived helper, raw samples and reproducible scripts retained. No benchmark-runner builds/tests overlap timing; unrelated host activity uncontrolled.

| Series | Case | Variant | Baseline us | Candidate us | Median paired gain | Positive processes |
|---:|---|---:|---:|---:|---:|---:|
| 1 | default_1k | 1 | 138.302 | 123.983 | +10.385% | 6/6 |
| 1 | default_1k | 2 | 138.302 | 135.443 | +2.091% | 6/6 |
| 1 | default_10k | 1 | 1358.463 | 1299.745 | +4.038% | 6/6 |
| 1 | default_10k | 2 | 1358.463 | 1352.057 | +0.385% | 6/6 |
| 1 | repeated_1k | 1 | 87.699 | 87.048 | -0.017% | 3/6 |
| 1 | repeated_1k | 2 | 87.699 | 87.897 | +0.212% | 5/6 |
| 1 | unicode_1k | 1 | 266.425 | 253.415 | +5.221% | 6/6 |
| 1 | unicode_1k | 2 | 266.425 | 264.398 | +0.899% | 6/6 |
| 1 | prefix_skew_1k | 1 | 62.772 | 62.067 | +0.697% | 5/6 |
| 1 | prefix_skew_1k | 2 | 62.772 | 62.709 | +0.008% | 4/6 |
| 2 | default_1k | 1 | 139.784 | 124.990 | +10.675% | 6/6 |
| 2 | default_1k | 2 | 139.784 | 136.767 | +2.098% | 6/6 |
| 2 | default_10k | 1 | 1355.521 | 1301.526 | +3.968% | 6/6 |
| 2 | default_10k | 2 | 1355.521 | 1359.417 | -0.335% | 2/6 |
| 2 | repeated_1k | 1 | 87.634 | 87.775 | -0.285% | 0/6 |
| 2 | repeated_1k | 2 | 87.634 | 87.555 | -0.006% | 3/6 |
| 2 | unicode_1k | 1 | 269.139 | 255.750 | +5.258% | 6/6 |
| 2 | unicode_1k | 2 | 269.139 | 266.741 | +1.068% | 6/6 |
| 2 | prefix_skew_1k | 1 | 63.946 | 63.604 | +0.534% | 6/6 |
| 2 | prefix_skew_1k | 2 | 63.946 | 64.295 | -0.546% | 2/6 |

| Corpus | Terms | Original capacity | Unconditional capacity | Adaptive capacity |
|---|---:|---:|---:|---:|
| default_1k | 1005 | 1792 | 1792 | 1792 |
| default_10k | 10005 | 14336 | 14336 | 14336 |
| repeated_1k | 2 | 3 | 1792 | 3 |
| unicode_1k | 1005 | 1792 | 1792 | 1792 |
| prefix_skew_1k | 38 | 56 | 1792 | 448 |

## Decision: do not accept

Adaptive preserves the constant-two-term capacity (3), unlike unconditional reserve (1792), but gains only ~2.1% (~3 us) CPU on default 1k. Default 10k is mixed +0.38% /-0.33%. Diverse-prefix/repeated-tail still grows capacity 56 ->448 for only 38 terms and does not establish a repeatable timing benefit. That capacity tradeoff and limited component return do not justify a production policy or engine benchmark for this unchanged heuristic. Public map capacity is not RSS; no resident-memory claim.

Exact maps match original helper for all measured fixtures. No source/API/format/sync changes, no workspace rerun for unchanged production, no end-to-end DB acceleration or peer victory. Full eight-peer objective remains open, including durable SQLite gap and missing live MSSQL/Kusto results. Future FTS work needs a different mechanism or stronger profile evidence; unchanged reserve heuristics should not be retried.
