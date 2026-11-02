# Crossed source-control diagnostic

216 processes / 1728 observations. Three labels per identical executable. Descriptive diagnostic only; original rejection remains binding.

| Case | Series | Cross-build gain % | Positive rounds | Baseline null gains ab/ac/bc % | Candidate null gains ab/ac/bc % |
| --- | --- | ---: | ---: | --- | --- |
| compare/agg_count_by/duckdb | series1 | -33.244 | 0/6 | -1.326/-3.074/0.135 | 1.594/0.781/-2.390 |
| compare/agg_count_by/duckdb | series2 | -34.675 | 0/6 | -0.926/-5.614/-0.495 | 11.631/3.983/0.542 |
| compare/durable_delete_1row_10k/sqlite_delete | series1 | -0.040 | 3/6 | -4.139/-9.142/0.315 | 0.486/5.323/1.756 |
| compare/durable_delete_1row_10k/sqlite_delete | series2 | -1.409 | 2/6 | -4.681/-2.097/1.777 | -6.448/-0.854/6.265 |
| compare/durable_commit_1row_1k/lin | series1 | 2.797 | 5/6 | -4.484/-2.652/-1.088 | 4.132/1.604/-1.587 |
| compare/durable_commit_1row_1k/lin | series2 | 4.286 | 5/6 | -2.010/-2.390/2.580 | 1.260/-4.334/-5.271 |
