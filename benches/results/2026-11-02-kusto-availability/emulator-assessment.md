# Kusto emulator is not a benchmark substitute

Checked 2026-11-01 UTC against Microsoft's documentation:

- [Kusto emulator overview](https://learn.microsoft.com/en-us/azure/data-explorer/kusto-emulator-overview): the emulator contains the query engine for local development and testing. Microsoft states that its license prohibits benchmark tests and that its performance profile differs from Azure Data Explorer.
- [Installation prerequisites](https://learn.microsoft.com/en-us/azure/data-explorer/kusto-emulator-install): ARM processors are unsupported; supported CPUs need SSE4.2/AVX2. Installation requires explicit license acceptance.

No emulator was installed, no license accepted and no measurements taken. Do not report emulator timings as Kusto superiority. The actual Azure Data Explorer comparison still needs an available service endpoint and database, with suitable authentication. Existing endpoint-availability evidence and the original eight-peer objective remain unchanged.
