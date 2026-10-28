# Equivalent point-values local baseline

Runtime a7d4b02c3; harness 63d8a7f53; macOS ARM64 nightly 1.101. Three fresh sequential processes, six cases, twelve samples, maximum eight operations per sample. Full value validation before calibration and after measurement. Input setup/drop excluded, output drop included. All raw cases retained.

This is a capped local diagnostic. Sub-microsecond operations, scheduler effects, and short samples limit precision. Numerical ratios are not performance qualification, evidence of production COW acceptance, or completion of the eight-peer goal. No A/A noise gate.

| Case | Peer | Lin/peer per process | Numerical wins |
|---|---|---|---|
| hit | sqlite | 0.3489, 0.3513, 0.3333 | 3/3 |
| hit | duckdb | 0.0088, 0.0111, 0.0114 | 3/3 |
| miss | sqlite | 0.1850, 0.2350, 0.1769 | 3/3 |
| miss | duckdb | 0.0045, 0.0042, 0.0038 | 3/3 |

All engines return owned id/title strings and preserve multiplicity. Legacy COUNT-based point_get results remain separate. These results do not cover PostgreSQL/MySQL/MongoDB/pandas/MSSQL/Kusto, durable writes, or the full engine control matrix.
