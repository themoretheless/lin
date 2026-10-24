# Candidate native peer refresh

Three processes ×12 samples, 1k/10k/100k; exact validation outside timing. Source unchanged, sentinel and cleanup checks passed; owned servers stopped. Native API contracts are not disk-durability equivalence. MSSQL/Kusto absent.

| Rows | Case | Peer | Lin ms | Peer ms | Speedup |
|---:|---|---|---:|---:|---:|
| 1000 | insert_native | sqlite | 0.762729 | 1.040375 | 1.364× |
| 1000 | insert_native | duckdb | 0.762729 | 1.161229 | 1.522× |
| 1000 | insert_native | postgres | 0.762729 | 4.620770 | 6.058× |
| 1000 | insert_native | mysql | 0.762729 | 8.333063 | 10.925× |
| 1000 | insert_native | mongo | 0.762729 | 5.080250 | 6.661× |
| 10000 | insert_native | sqlite | 8.139979 | 12.934021 | 1.589× |
| 10000 | insert_native | duckdb | 8.139979 | 9.740250 | 1.197× |
| 10000 | insert_native | postgres | 8.139979 | 27.406479 | 3.367× |
| 10000 | insert_native | mysql | 8.139979 | 82.968396 | 10.193× |
| 10000 | insert_native | mongo | 8.139979 | 47.569666 | 5.844× |
| 100000 | insert_native | sqlite | 92.743979 | 144.702645 | 1.560× |
| 100000 | insert_native | duckdb | 92.743979 | 103.192729 | 1.113× |
| 100000 | insert_native | postgres | 92.743979 | 283.770167 | 3.060× |
| 100000 | insert_native | mysql | 92.743979 | 882.706417 | 9.518× |
| 100000 | insert_native | mongo | 92.743979 | 570.301750 | 6.149× |
