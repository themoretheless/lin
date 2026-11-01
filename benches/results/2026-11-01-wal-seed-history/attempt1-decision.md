# Single seed transaction prototype incomplete

17 functional probes passed. All 16 cases at 1k rows passed. The 10k chunked Lin UPDATE passed; the next case, 10k grouped Lin UPDATE, failed during seed commit with `persist: log record exceeds 16MiB`. The target mutation was never reached for that case; the remaining 14 probes were not executed. No latency observations or performance qualification were produced.

The original fixture streams separate 500-row insert programs, whereas grouped mode puts all programs into one interactive transaction. The grouped log record exceeds the existing cap; the exact encoding/byte breakdown has not been measured here. Do not infer an exact byte size from the error. The cap, frame format, CRC and recovery remain unchanged. All original outputs and the compiled harness provenance are preserved in local-functional-attempt1.tar.gz, with per-member hashes in attempt1-status.json. The original fsync experiment is separate and still uses the unchanged fixture.

Future counterfactuals must explicitly bound grouped transaction size or adopt a separately designed atomic multi-frame transaction format. Either would be a new experiment, not a retry of this failed single-transaction setup or proof of the cause of the SQLite latency gap.
