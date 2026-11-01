# WAL frame boundary I/O diagnostic

Hypothesis only: determine whether frame length and starting offset affect ordinary write_all plus sync_data cost on Linux. Rust sync_data on Linux preserves the existing fdatasync contract. No production source, durability setting or WAL format changes.

Frozen lengths: 117, 4095, 4096, 4097, 4125, 8192 bytes. Offsets: 0, 2048, 4095 bytes. Six rounds, rotated 18 cases and two identical-binary roles, eight samples each: 216 fresh processes, 1728 observations. Each sample creates a fresh file, writes and syncs the prefix before timing, times write plus sync, then checks every byte and reopens outside timing. Record filesystem metadata, exact commands, binary and source hashes; retain files and all outcomes. This measures fresh file growth at declared offsets, not the complete append history of a database. No peer or source acceptance claim.

Local execution is functional only with one sample per case. Performance matrix is Linux x86_64 only; never retry unchanged to obtain a preferred outcome.
