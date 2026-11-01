# Corrected COW durable phase diagnostic

Diagnostic only; no source-candidate acceptance and no eight-peer superiority claim. Use the exact corrected COW source hashes in source-input.json. Run full release tests on instrumented source, restore production files, then build separate ordinary and instrumented executables in a dedicated target directory.

Ten cases: durable UPDATE, DELETE, COMMIT, ROLLBACK at 1k and 10k, plus scalar Full and Normal UPDATE. Thirty functional smokes must verify full-value reopen and expected phase counts. Measurement is Linux x86_64 only, six rounds, rotated case and role order, four roles (ordinary, same-binary control, instrumented disabled, instrumented traced): 240 fresh processes, eight observations each, 1920 total. Preserve every process and failure; no unchanged retry. Compare disabled instrumentation overhead and traced overhead separately. Caller-thread phase clocks are nested where applicable; do not sum overlapping phases or transfer earlier-source percentages.

Before dispatch freeze scripts, generated overlays, runtime, Cargo inputs, helper and workflow hashes. Current test count requirement: 458 passed, two ignored. Current Kusto and private MSSQL limitations remain unchanged.
