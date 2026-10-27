# Writer lock release after fork inheritance

Baseline main205 reproduced a stale writer flock in all six processes (baseline and COW): close succeeded, immediate reopen failed while the child held inherited descriptors, and reopen succeeded after child exit. See ../2026-10-27-writer-lock-inheritance/.

The fix explicitly unlocks when Persist drops, only in the process that acquired the lock. The child exercises the same release helper without allocating or dropping heap objects after fork. The parent remains exclusively locked until its own Persist drops, then reopens while the child remains alive.

Validation: 406 default library/integration tests passed, 2 existing ignored. The fork regression passes fixed and fails both mutants (missing explicit release; missing owner-PID guard). Original engine reproducer passes three independent processes with exact persisted data readback. Logs and statuses retained. This does not prove forked engine use is supported, nor uniquely attribute the earlier Linux CI failure to descriptor inheritance.

The next paired Linux input applies this fix identically to baseline and COW-inplace candidate. Performance acceptance is pending; no new throughput or peer win is claimed here.
