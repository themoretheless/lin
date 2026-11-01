# Balanced column Batch WAL: rejected

Run 41353571509; frozen c73b1af46b4b9957d97a2b1dfb9958bc47546f65. 864 fresh processes / 6912 observations. All 4710 archived files rehashed; local reconstruction matches CI. Per-case role positions, precedence and adjacency balance verified. Linux executable payloads are not archived.

Full release suite, lost-child negative, 32 value probes, production reader values/f32 checks, one-bit negative, 48 smokes and independent raw/source audit passed. Only the unchanged acceptance gate failed.

| Target | Series | Gain % | A/A % | Positive |
| --- | --- | ---: | ---: | ---: |
| compare/column_batch_full_insert_1k/lin | series1 | 82.467 | 3.618 | 6/6 |
| compare/column_batch_full_insert_1k/lin | series2 | 82.400 | 3.122 | 6/6 |
| compare/column_batch_full_insert_2k/lin | series1 | 79.070 | 1.372 | 6/6 |
| compare/column_batch_full_insert_2k/lin | series2 | 79.209 | 1.810 | 6/6 |

All four target windows pass. Two control windows fail:

- compare/durable_update_1row_10k/lin series1: gain -3.882%, A/A 3.564%, 1/6 positive.
- compare/update_1row_10k/lin series2: gain -5.234%, A/A 2.665%, 2/6 positive.

Warm UPDATE does not execute WAL persistence. Its regression cause is unproven; no noise waiver or causal attribution.

SQLite FULL WAL: 0/96 numeric wins. Grouped insert Lin/SQLite ratios: 1.496603 at 1k, 1.869497 at 2k. Single-row mutation ratios: 1.189467 to 1.885262. Native workloads retain their declared physical schema differences.

Do not adopt or rerun unchanged. Original unbalanced run remains failed and separately archived. Full source qualification and eight-peer superiority remain open.
