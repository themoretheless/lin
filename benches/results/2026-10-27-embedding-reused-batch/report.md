# Reused joined text with one hashing scratch borrow per batch

Distinct mechanism from rejected per-row streamed embedding: candidate reuses a String across rows and enters EMBED_SCRATCH once for the whole batch. Baseline calls actual production row_embed_text then public embed_batch; candidate uses unchanged actual fill/lowercase/finish inside a temporary diagnostic helper. No duplicated hash/normalization implementation or alternative table. Public diagnostic exports and helpers existed only during build; source restored and checked. No production API extension or Db dispatch implemented.

Exact vector-bit preflight covers125 absent/empty/short/Unicode/long field combinations plus a long repeated text to exercise normalization fallback; dimensions8/9/128/768/1536. Actual vertical tab and nonbreaking space included. Every benchmark fixture is checked for exact output before timing. Empty rows are embedded on both component paths; this does not qualify Db missing/existing-embedding filtering or third-party batch overrides.

Six independent release processes,12 rotated A/B samples per case/process. Same model and scratch instance; initial equivalence calls warm both paths. Text preparation, output allocation/destruction included; fixture/model construction excluded. No A/A calibration or independent second series, no isolated-host guarantee.

| Case | Median paired gain % | Faster processes |
|---|---:|---:|
| default_10k | 8.488 | 6/6 |
| default_1k | 10.234 | 6/6 |
| single_field_1k | -0.210 | 2/6 |
| unicode_1k | 0.942 | 6/6 |

Promising default-shaped component gain; not accepted as an engine optimization. Single-field control has a small negative median and only2/6 wins; its significance is unresolved without A/A. Do not compare causally to the older streamed experiment on another source/host state. Next: calibrated two-series A/A/B component gate with all controls retained, then backward-compatible custom Embedder batch fallback and full-engine native/durable qualification if justified. No SQLite/eight-peer win or production change. Raw observations and source/helper/binary hashes retained.
