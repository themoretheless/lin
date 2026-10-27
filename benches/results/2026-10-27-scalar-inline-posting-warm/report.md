# Scalar singleton inline posting: allocation reduction, no warm speed qualification

Baseline previous Map/static-output prototype; candidate only changes scalar PostingList::Standard backing Vec to SmallVec<[usize;1]> plus required conversions preserving existing Vec-returning APIs and exact intersection/union behavior. This is independent of old rejected inline FTS and of held reverse-entry experiment. Index module is private; stable public Row alias preserved. No wire/model/sync change. Full downstream nested-type compatibility and broader all-features qualification not established.

415 default library/integration tests passed,2existingignored, includes existing prototype oracles and experimental mutation_contract target. Additional retained6144-step membership model covers unique/partial/conflicts,1/2/3/5-part keys, spill and bitmap promotion; bitmap emptiness oracle covers sparse/growth/final removal. No new induced-fault sensitivity was run specifically for this representation; prior held-entry mutant evidence must not be presented as this candidate sensitivity.

Measured layout baseline/candidate32bytes, no enum-size growth. Baseline standard capacity0/4/4/4/4 for lengths0/1/2/3/4; candidate1/1/2/4/4. Thus third-id growth adds a potential extra allocation/reallocation relative to baseline; no across-cardinality allocation or memory/RSS superiority claimed. Exact layout stdout retained in ../2026-10-27-scalar-inline-posting/.

Allocation diagnostic ../2026-10-27-scalar-inline-posting-allocation/:3fresh processes per variant,9cells each=54observations; all repetitions identical. wing UPDATE saves1allocation and32requested bytes at each size. Title UPDATE and DELETE unchanged. Full Handle drop and output retained. Requested bytes are not retained memory or RSS.

18rotated A/A/B warm release processes,6rounds,1728observations.16batches of512alternating updates and16explicit rollback observations per size/process. Source restored; source/binary hashes retained.

| Rows | UPDATE gain % | Absolute A/A % | Positive pairs | Rollback gain % | Absolute A/A % |
|---|---:|---:|---:|---:|---:|
| 1000 | -0.006 | 2.003 | 3/6 | 0.297 | 0.667 |
| 10000 | 0.402 | 1.479 | 3/6 | 0.983 | 1.635 |
| 100000 | -1.810 | 4.181 | 1/6 | 3.515 | 3.629 |

Do not promote from this evidence: UPDATE remains within A/A with inconsistent signs at all sizes; no stable warm speed gain. This does not prove a fresh cold latency regression or superiority. Native/durable/read/platform and all-cardinality allocation guards remain unmeasured; no peer win or eight-engine completion. Allocation reduction alone cannot qualify the policy.

Next diagnosis should reproduce actual failed-candidate phase costs on the Linux host where0/120SQLite wins occurred, rather than extrapolate macOS phase times or warm lookup kernels. Existing Map/static source remains the profile subject, not this unqualified posting variant.
