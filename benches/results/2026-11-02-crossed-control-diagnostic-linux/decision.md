# Crossed source-control result

Run 41363558777 completed successfully: 216 processes / 1728 observations; 134 prior full-harness functional smokes; source, binary identity, exact command, balanced-order and numerical audit passed. Local reconstruction matches CI. All 2075 raw archive files rehashed. Three labels execute the identical baseline ELF; three labels execute the identical candidate ELF. Both downloaded Linux ELF64 x86_64 files match their recorded live hashes. Executables are retained in the separate CI artifact and locally, not embedded in Git.

DuckDB GROUP BY has a negative cross-build gain in both series: -33.244% and -34.675%, 0/6 positive rounds each. This means candidate executable query latency was about 33-35% greater. Null-label median contrasts are also retained, including candidate ab +11.631% in series2. This descriptive experiment does not provide a formal uncertainty interval or identify a physical cause.

SQLite DELETE 10k contrasts: -0.040% / -1.409%. Lin FULL COMMIT 1k: +2.797% / +4.286%, 5/6 positive rounds each. The earlier failures of those controls remain failed; they are not retrospectively accepted or labelled noise.

The DuckDB SQL and callback source are unchanged across builds; no Lin Row conversion occurs inside this query callback. That alone does not establish equal compiled instructions, code/data layout, allocator state or CPU/cache behavior. Exact executables now make further static inspection possible. No cause is asserted from these observations.

The source gate from run41353987944 remains rejected. No performance qualification, source adoption or unchanged acceptance retry. The eight-peer objective and SQLite FULL WAL gap remain open.
