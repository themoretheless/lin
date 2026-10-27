# Held reverse-map Entry: no qualified improvement

Baseline is previous Map/static-output prototype (failed Linux40448137140), not production main. New candidate retains one reverse HashMap Entry across forward membership maintenance, borrows old key from the occupied entry, preserves unique precheck and same-key early return, and writes canonical forward-entry key once through that slot. This differs from the existing correctness-only remove/insert single-write prototype: it does not remove/reinsert the reverse slot and eliminates repeated reverse lookup rather than merely changing write count. Public types/layout and semantics unchanged.

Default cargo test --lib --tests:413passed,2existingignored, includes prior prototype oracles and experimental mutation_contract target. Additional6144step membership oracle passes across arities1/2/3/5 xunique/nonunique xpartial/full; model rebuilds expected forward posting sets and reverse memberships after every operation, checks conflict atomicity. Dense low-cardinality postings and spilled/long-text keys included. Missing old-posting removal and missing vacant reverse insertion mutants each fail assertions. Logs/exact test sources in ../2026-10-27-index-reverse-held-entry/. Source restored after each test/build/run.

18rotated baseline/control/candidate warm release processes,6rounds,1728observations.16batches of512alternating updates plus16explicit rollback observations per size/process; existing generation/field/index-count checks retained. Source/binary hashes and every process/raw sample retained. This is a warm local diagnostic, not fresh Linux or peer/Full-durable qualification.

| Rows | UPDATE gain % | Absolute A/A % | Positive pairs | Rollback gain % | Absolute A/A % |
|---|---:|---:|---:|---:|---:|
| 1000 | 0.410 | 2.661 | 3/6 | -3.694 | 4.226 |
| 10000 | -1.613 | 1.845 | 2/6 | -0.640 | 2.866 |
| 100000 | 2.129 | 1.330 | 5/6 | 1.923 | 2.271 |

Do not promote:1k gain is below A/A with3/6positive,10k negative median,100k only5/6positive. This does not show that reverse lookup/write count explains the cold scalar-maintenance bottleneck. No unchanged Linux retry seeking green, no peer victory or eight-engine completion. Prototype remains archived; production sources unchanged.
