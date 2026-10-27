# COW full lifecycle heap-allocation diagnostic

Main205 + owner-PID writer-lock release versus the frozen COW-inplace candidate with the same lock fix. macOS ARM64 debug binaries, three fresh processes per size and variant, 18 processes. This measures allocator layout requests and live requested bytes, not RSS, timing or release-build performance. Counting is continuous, including setup, output result retention and destruction. Global initialization/cache retention is reported and is not labelled a leak. Fixture: indexed docs with body/hash/embedding, fixed timestamps, exact every-row comparison against Store.

Full-row output uses explicit `take N`, with the same declared 1 GiB output quota in both variants. Original default quota rejects 100k output at 370785560 estimated bytes; that failure is retained. The initial `docs` harness returned the documented default 50 rows. An explicit `docs | take all` also returned only 50 because scan prefetch ignores the explicit unlimited take; its failing evidence is retained separately and requires an engine fix. No default-quota or take-all success is claimed. A failed attempt to set the private quota field is retained; the final harness uses the public with_quotas API.

| rows | seeded live bytes baseline | candidate | delta | full-query live bytes baseline | candidate | query allocations baseline | candidate |
|---|---:|---:|---:|---:|---:|---:|---:|
|1000|5274755|5298755|24000|606942|8942|10093|93|
|10000|50588131|50732131|144000|6060944|80944|100093|93|
|100000|494780490|496732490|1952000|60600946|800946|1000093|93|

Medians across three processes; all raw process values and paired deltas remain in summary.json. COW adds storage/initialization cost while sharply reducing full-row result copies. This does not qualify peer speed or overall memory superiority. Linux mutation timing acceptance remains a separate pending gate.
