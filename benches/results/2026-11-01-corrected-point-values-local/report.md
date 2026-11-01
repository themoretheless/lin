# Corrected COW: full-value point lookup local comparison

Runtime625bc7180 (corrected COW + CAS + UPDATE); byte-identical point_values harness; macOS ARM64 nightly 1.101. Three fresh sequential processes, six cases, twelve samples, maximum eight operations per sample. Full value validation before calibration and after measurement. Input setup/drop excluded, output drop included. All raw cases retained.

This is a capped local diagnostic. Sub-microsecond operations, scheduler effects, and short samples limit precision. Numerical ratios are not performance qualification, evidence of production COW acceptance, or completion of the eight-peer goal. No A/A noise gate.

| Case | Peer | Lin/peer per process | Numerical wins |
|---|---|---|---|
| hit | sqlite | 0.5326, 0.6091, 0.4609 | 3/3 |
| hit | duckdb | 0.0137, 0.0145, 0.0130 | 3/3 |
| miss | sqlite | 0.2976, 0.3082, 0.2654 | 3/3 |
| miss | duckdb | 0.0051, 0.0053, 0.0045 | 3/3 |

All engines return owned id/title strings and preserve multiplicity. Legacy COUNT-based point_get results remain separate. These results do not cover PostgreSQL/MySQL/MongoDB/pandas/MSSQL/Kusto, durable writes, or the full engine control matrix.
