# Default embedding dimension modulo specialization — rejected

Candidate explicitly selects constant modulo 768 for the default dimension, retaining dynamic modulo for other sizes. Slot semantics unchanged. Full workspace tests including bit-exact embedding references pass.

Six alternating process pairs; 24 fresh fixtures per case, one operation. Full native Lin, SQLite, DuckDB Appender paths with exact row checks outside timer. Schema/setup/drop excluded. Pinned optimized binaries; builds/tests did not overlap measurement. Host load uncontrolled; case order fixed. Milliseconds.

| Pair | Lin 1k baseline | candidate | Lin 10k baseline | candidate |
|---:|---:|---:|---:|---:|
| 1 | 1.271396 | 1.118667 | 16.906645 | 11.590874 |
| 2 | 1.077666 | 1.049708 | 11.540417 | 11.624645 |
| 3 | 1.072583 | 1.074730 | 11.435104 | 11.554604 |
| 4 | 1.069605 | 1.082187 | 11.507021 | 11.785542 |
| 5 | 1.070771 | 1.065041 | 11.448688 | 11.520270 |
| 6 | 1.063167 | 1.075562 | 11.460646 | 11.574730 |

1k candidate wins 3/6 (median paired decrease 0.17%); 10k wins only 1/6 (median decrease -0.86%, slower). First pair shows large baseline variation shared with SQLite. No stable improvement established. Candidate rejected, original embedding source restored. All previously retained WAL changes remain. Native current-state comparison recorded separately in ../2026-10-01-native-after-wal/report.md. All-eight-peer goal remains unproven.
