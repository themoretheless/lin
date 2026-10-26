# Common mutation completion diagnostic

Main205, release macOS ARM64, three independent processes, nine fresh fixtures each; all 27 exact data/index checks pass. Temporary source instrumentation restored exactly. Counting allocator and nested Instant spans perturb timings; medians are diagnostic, not accepted performance or peer evidence. Full raw logs and summary.json retained.

Added inclusive spans: check_quotas (14), exec_program (15), main Db::run_prepared (16). Original 0..13 mapping matches v2. Span totals exclude their own RAII Drop and caller Handle drop; nested values must not be summed.

At 100k wing UPDATE: mutation 10000 ns, program 10917 ns, prepared 12000 ns. DELETE: mutation 13584 ns, program 14083 ns, prepared 15500 ns. Quota check medians 0..42 ns across cases (timer resolution matters). Thus no evidence of an expensive total-row quota scan or dominant common commit wrapper; most observed time remains in mutation maintenance. This rejects spending another optimization round on check_quotas based only on suspicion.

No production change or speed claim. Next profile must split row clone/undo, CAS validation, FTS removed-row maintenance and scalar index key maintenance; neither previously rejected FTS candidate nor borrowed-row candidate is accepted by these observations. Broad eight-peer goal remains open.
