# Inline two-cell UPDATE hypothesis

Baseline: lazy COW core from da8e4ec26 (identical core to frozen Linux 00c8aa9a3). Candidate changes only UPDATE promotion: maps with at most two cells stay inline. Existing shared rows retain normal COW isolation; wider rows retain promotion. No serialization, sync, quota, WAL format or public API changes.

Motivation: the complete Linux gate failed three scalar WAL control windows. Its scalar fixture has id/value cells and no vectors. Allocating an Arc for every fresh two-cell update may cost more than map clones. This is a hypothesis, not an established cause.

Before acceptance: full release correctness suite; fresh paired same-harness native scalar and Full/Normal WAL timing with complete reopen/readback and unchanged wide UPDATE controls. A component win alone does not qualify the 67-case full engine or eight-peer objective. Preserve rejected observations; restore baseline if the hypothesis fails.
