# Linux WAL reserve: transaction-corrected functional revision

Base 4cbc5275ed1cdfbb85b41a64ea4053c8ac38c7de includes the transaction WAL fix and subprocess recovery tests. Production persistence source remains unchanged. Candidate persistence source is identical to the previous bounded Linux KEEP_SIZE prototype; no new latency hypothesis is added.

Corrected the independent frame oracle from IEEE CRC32 to Castagnoli CRC32C using a bitwise reference (0x82f63b78). The rollback/follower/checkpoint assertions are retained. Previous failing Linux artifact remains preserved separately.

Local macOS full default release suite: 417 passed, 2 ignored. Portable range model includes 1056 independent wide-arithmetic cases. A deliberately wrong zero reservation offset fails both geometry tests at runtime. Source restoration and input hashes verified. Actual Linux reserve/replay tests are cfg-disabled here and require CI; this local result does not qualify Linux behavior or latency.

The prior transaction-source CI remains live independently. This branch runs functional candidate qualification only. Full durable comparison against SQLite WAL remains a separate required performance gate after correctness passes.
