# Microsoft peer qualification and Kusto cleanup fix

Checked 2026-10-24 on macOS arm64 / Docker aarch64. MSSQL and Kusto configuration variables are absent in this process environment (booleans only captured, no credential values or unrelated credential search). No Microsoft peer performance result is claimed. No emulator downloaded or started.

## Why a local emulator cannot close the gap

Microsoft's [Kusto emulator overview](https://learn.microsoft.com/en-us/azure/data-explorer/kusto-emulator-overview) states that benchmark testing is prohibited by the emulator terms and that its performance profile differs from Azure Data Explorer. Therefore it is not a substitute for the requested Kusto performance comparison. The [installation requirements](https://learn.microsoft.com/en-us/azure/data-explorer/kusto-emulator-install) also exclude ARM processors.

The [SQL Server container documentation](https://learn.microsoft.com/sql/linux/quickstart-install-connect-docker) supports Intel/AMD x86-64 Linux hosts and states that emulation/translation is untested and unsupported. This is not proof that an emulated container can never run; such a result would require separate qualification and would not establish native peer performance. A supported test instance is the remaining route for this comparison. Existing read adapters still need live exact-result validation against both services before their times can count.

## Implemented adapter correction

Kusto.close previously stopped at the first failed DROP, leaving later task-owned tables unattempted. It now attempts every table it created in reverse order, closes the HTTP connection, then propagates the first DROP exception so that cleanup failure still marks the run incomplete. It never enumerates/deletes unrelated tables. A server rejecting DROP can still leave its affected table: the correction guarantees attempts and explicit failure, not successful cleanup against an unavailable service.

The new injected-failure regression failed before the fix (only one of three drops attempted). After the fix all three drops are attempted despite two failures, the connection closes, and the error propagates. All ten adapter tests passed, including actual SQLite/DuckDB fixture/readback tests, independent-process aggregation/incomplete gates and Kusto partial-query-error rejection. Kusto failure injection is mock evidence, not live service validation. Pre-fix and passing logs retained; Python-only change does not require rerunning the unchanged Rust workspace or performance matrix.

## Remaining full objective

Current six-peer read matrix and five-peer native insertion evidence remain source-specific and unchanged in the Rust engine. Durable 1k SQLite gap remains; MSSQL/Kusto live coverage and their applicable write contracts remain missing. This correction improves future benchmark reliability rather than database throughput or a peer win.
