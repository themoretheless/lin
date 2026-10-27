# Completed Linux schema crash/HEAD qualification

[Run40536991671](https://github.com/themoretheless/lin/actions/runs/40536991671), head9fdbe74649966442306e9f859241e9989cbbb443,native Ubuntu24.04 x86_64,Rust1.96.0,completed successfully. Full default release library/integration suite:**430 passed,2 ignored,zero failed**.

All10 targeted schema recovery tests pass,including4 actual child-process exits after committed schema/row writes,2 modeled publication/generation states and3 negative metadata controls. Four SCHEMA_HEAD_PROBE records cover autocommit/transaction × no snapshot/prior checkpoint;all reopen_error values are null. Independent verify.py checks the complete log,count,named cases,probe combinations,and exact historical store/exec/regression Git hashes. Existing transaction abort,WAL generation and subprocess recovery regressions also pass.

This is correctness validation,not a power-loss/filesystem persistence proof or performance acceptance. Publication order,sync calls,file formats and checkpoint interval are unchanged. The later available-peer API refresh uses core/helper bytes exactly matching this successful source;its distinct macOS/API timings do not qualify the older frozen Linux KEEP_SIZE candidate.
