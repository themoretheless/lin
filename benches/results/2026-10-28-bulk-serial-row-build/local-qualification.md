# Local correctness qualification

449 release library/integration tests passed, 2 ignored. The existing threshold/order/generated-ID/shared-time regression passed for 4095/4096/4101 rows. The complete release suite includes duplicate id/uri rejection across the former split boundary, no publication on validation error, held-result isolation, transaction/crash recovery and vector/FTS/index checks.

A temporary mutant passed only the first half of the records to the row builder. The threshold regression failed at runtime (0 passed /1 failed), and all candidate source hashes were restored exactly. Full positive suite ran before this temporary mutation; no production mutation retained. Logs and exact mutant source are preserved.

This establishes bounded correctness, not performance. No latency/CPU/RSS claim or production adoption. Planned timing acceptance remains in protocol.md; complete engine and eight-peer qualification remain required.
