# native candidate peer refresh

Three independent processes ×12 samples; 1k/10k/100k, exact validation, source stability, sentinel preservation, cleanup and stopped owned servers verified. Native API contract is not disk durability equivalence. MSSQL/Kusto absent; pandas only in read scenarios.

| Rows | Case | Peer | Lin ms | Peer ms | Speedup |
|---:|---|---|---:|---:|---:|
| 1000 | insert_native | sqlite | 0.758042 | 1.025688 | 1.353× |
| 1000 | insert_native | duckdb | 0.758042 | 1.148354 | 1.515× |
| 1000 | insert_native | postgres | 0.758042 | 4.674437 | 6.166× |
| 1000 | insert_native | mysql | 0.758042 | 8.394001 | 11.073× |
| 1000 | insert_native | mongo | 0.758042 | 5.011333 | 6.611× |
| 10000 | insert_native | sqlite | 7.982979 | 12.670208 | 1.587× |
| 10000 | insert_native | duckdb | 7.982979 | 9.598479 | 1.202× |
| 10000 | insert_native | postgres | 7.982979 | 26.729312 | 3.348× |
| 10000 | insert_native | mysql | 7.982979 | 80.852771 | 10.128× |
| 10000 | insert_native | mongo | 7.982979 | 47.445687 | 5.943× |
| 100000 | insert_native | sqlite | 96.002229 | 149.387916 | 1.556× |
| 100000 | insert_native | duckdb | 96.002229 | 101.426104 | 1.056× |
| 100000 | insert_native | postgres | 96.002229 | 313.663063 | 3.267× |
| 100000 | insert_native | mysql | 96.002229 | 858.801522 | 8.946× |
| 100000 | insert_native | mongo | 96.002229 | 626.788854 | 6.529× |
