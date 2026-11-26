# Kusto inline ingestion and Lin prepared INSERT

Three fresh processes and three samples per engine and size. Lin used its local in-memory API with default embeddings. Kusto used explicitly selected inline management commands, timed through full visibility including network and polling. Full six-field readback was validated separately for every sample. Streaming ingestion remains disabled on this cluster.

| Rows | Lin ms | Kusto ms |
|---:|---:|---:|
| 1000 | 0.352 | 559.546 |
| 10000 | 4.218 | 1334.706 |
| 100000 | 28.861 | 5751.459 |
