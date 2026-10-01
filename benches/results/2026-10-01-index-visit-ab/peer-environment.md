# Remaining peer environment requirements

Current local host reports arm64. Benchmark connection variable presence is recorded as booleans only in peer-env-presence.json; no secrets were captured. These are environment observations, not a claim that no other credentials exist anywhere.

Microsoft SQL Server Linux containers support Intel/AMD x86-64 Linux hosts, while Rosetta/QEMU translation is not supported or tested: https://learn.microsoft.com/en-us/sql/linux/quickstart-install-connect-docker?view=sql-server-ver17

The Kusto emulator requires SSE4.2/AVX2 and does not support ARM: https://learn.microsoft.com/en-us/azure/data-explorer/kusto-emulator-install

The Kusto emulator has a different performance profile from Azure Data Explorer: https://learn.microsoft.com/en-us/azure/data-explorer/kusto-emulator-overview

Therefore local emulator timings on this host cannot close the live MSSQL/Kusto comparison requirement. A suitable native test environment and dedicated benchmark database connection are still required. The existing peer harness accepts these configurations. This gap does not block optimization against available peers and does not close or narrow the eight-peer objective.
