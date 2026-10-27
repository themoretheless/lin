# Linux qualification: failed interactive transaction cannot commit

[Run40531748030](https://github.com/themoretheless/lin/actions/runs/40531748030), frozenedf799b8b9cda3a6f70707d6d1ca2eb3014b52a1,native Ubuntu24.04 x86_64,Rust1.96.0,completed successfully. Full default release library/integration suite:**420 passed,2 ignored,zero failed**.

The five dedicated quota/CAS/parse failure regressions pass,including an error after an already accepted pack,subsequent prepared-run/prepare rejection,explicit rollback/Drop and next-write continuity. Existing transaction generation,atomic-frame,truncation and subprocess crash recovery tests also pass. Both TX_QUOTA_PROBE records show commit refused with an aborted error,the original wing=rag,generation1 and zero new WAL bytes. No quota or sync threshold was relaxed.

Independent verify.py checks exact frozen exec/test hashes through Git objects,counts and named regression results,and both concrete quota-failure records. Complete log and run metadata are retained. This proves this correctness scope on Linux; it is not a benchmark win or automatic qualification of the older frozen KEEP_SIZE timing source.
