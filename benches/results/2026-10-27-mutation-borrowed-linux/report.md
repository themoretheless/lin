# Borrowed-row candidate: Linux qualification not accepted

Run40394312683 on frozen6ac970ef completed all36 benchmark processes: two six-round series, rotated A/A/B,20native mutation/transaction cases including SQLite,8samples/case,5760timed operations. Candidate test/contract checks passed; raw benchmark validation, full source snapshots and binary hashes retained. Independently recomputed all240case-pairs from raw samples. No observations discarded.

| Lin target | Series1 gain / pairs / A-A | Series2 gain / pairs / A-A |
|---|---|---|
| update1k |+6.005% /5of6 /0.984%|+7.037% /6of6 /2.146%|
| update10k |+4.298% /4of6 /1.006%|+1.972% /5of6 /1.523%|
| update100k |+3.797% /5of6 /3.065%|+4.213% /4of6 /3.176%|

The predeclared gate requires6of6 target wins exceeding A/A in both series and unchanged controls within its bounds. It returned candidate_qualified=false. For example, delete100k series2 regressed2.533% versus A/A2.230%; SQLite transaction rollback10k series1 regressed6.807% versus A/A2.573%. Full table is artifact/paired-report.md, all controls/losses preserved.

All120 candidate same-case comparisons against SQLite lose. Reduced allocations are corroborated independently but do not close the actual objective. No production rollout, no accepted full-durable speedup, no eight-peer superiority claim. The previous local noisy attempt and initial ANSI harness failure remain separate retained experiments.

Next mechanism: prototype30ee3dd6 additionally transfers the original Row into undo ownership (30allocations saved rather than20), with278tests and54allocation-counter operations already passed. Qualify it separately; do not reinterpret or weaken this failed gate.
