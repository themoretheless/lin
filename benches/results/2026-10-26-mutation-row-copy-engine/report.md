# Rejected native mutation payload elision

Rejected before performance qualification. The candidate avoided a cloned Pack payload on in-memory update/delete, but native transactions use Pack.byte_size for max_journal_bytes and programs ending in an empty read use affected-row counts from the folded Pack. Empty write markers weaken these contracts.

Two dedicated regressions in tests/mutation_contract.rs pass against original code and fail against this candidate; sensitivity verified in ../2026-10-26-mutation-borrowed-row-engine/contracts-rejected.log. The same regressions pass against the revised borrowed-row-only candidate. Initial regression fixture used unsupported semicolon separators and produced a compile error; corrected to newline-separated statements before functional conclusions.

Initial attempt to borrow the before Row through mutable maintenance failed Rust borrowing checks; failed-borrow source/log/provenance retained. The revised ownership draft compiled, but contract review rejected payload elision. Only control/candidate first runs completed; orchestration was explicitly terminated and the subsequently running baseline was not qualified. No partial timing supports acceptance. Original source restored; no production rollout.
