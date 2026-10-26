# Bitmap emptiness CPU: do not enable as a general speedup

Frozen production core20591dd4; saved candidate delegates PostingList emptiness to Vec/BitSet emptiness and replaces two index-edit popcount-zero checks. No layout/format change. Twelve processes, two six-process series, rotated A/A/B,24samples×512checks per case;9cases,3981312timed predicate checks. Boolean-column index fixtures have1000/10000/100000rows; dense/late-only/empty memberships checked against exact index positions. Allocation/drop/setup excluded. Original source restored after build; raw samples/binary hash retained.

Dense predicates improve, but late-only/totally-empty guards regress. First process: dense1k2.28→1.38ns,10k27.18→1.38ns,100k217.37→1.38ns; empty1k2.20→3.34ns,10k27.18→44.47ns,100k217.90→288.57ns. These are component observations on ARM, not Linux end-to-end mutation evidence. Full two-series data in summary.json; no samples discarded.

Do not ship this as a general empty-check optimization or infer a closed SQLite gap from the dense case. Absolute savings are at most hundreds of ns here, much smaller than the measured mutation gap. Next higher-leverage hypothesis is fusing FTS deletion with the swap-moved row to avoid redundant common-term edits; qualify its semantics before timing.
