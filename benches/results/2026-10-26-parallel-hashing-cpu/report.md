# Scoped parallel hashing CPU feasibility — rejected

Frozena5e28e36, exact public HashingEmbedder serial batch API versus a two-half
prototype with one scoped thread. Eligibility:512..16384 rows,512Ki..16Mi total
float slots, first text byte length times row count>=8KiB. Routing only; every
output is checked against the original serial provider by raw f32 bits.

Two independent series of six release processes.24 rounds/case rotate all six
orders of serialA/serialB/parallel.10 cases,8640 timed compositions. Inputs and
reference outputs prepared outside timing. Output destruction excluded; cold
thread creation/join/TLS allocation/destruction included. A forced no-spawn
branch is checked outside timing; this is not a real OS resource-failure test.
No engine/WAL/durability/peer benchmark and no production change.

| Case | Paired gain s1 / s2 (%) | Faster processes s1 / s2 | A/A median absolute difference s1 / s2 (%) |
|---|---:|---:|---:|
| default_1k | -17.890 / -74.315 | 1/6 / 0/6 | 0.846 / 1.271 |
| default_10k | -6.134 / -38.690 | 3/6 / 1/6 | 2.959 / 2.158 |
| boundary_682 | -0.307 / +0.088 | 2/6 / 3/6 | 0.298 / 0.158 |
| boundary_683 | -22.446 / -39.754 | 0/6 / 0/6 | 0.483 / 1.796 |
| boundary_512_dim1024 | -7.505 / -137.054 | 2/6 / 0/6 | 0.769 / 4.195 |
| guard_dim64 | +0.013 / -0.051 | 3/6 / 0/6 | 0.142 / 0.093 |
| guard_16500 | +0.585 / +0.798 | 4/6 / 6/6 | 1.044 / 0.619 |
| short_repeated | +0.104 / -0.851 | 4/6 / 1/6 | 0.339 / 0.653 |
| long_unicode | +18.781 / +19.392 | 4/6 / 4/6 | 2.577 / 4.489 |
| skewed_lengths | -126.164 / -107.705 | 0/6 / 0/6 | 1.539 / 1.787 |

Reject the broad small-batch parallel policy: ordinary1k/10k and eligible small
boundaries regress, beyond their A/A noise. Skewed text lengths regress by more
than100%; long Unicode gains do not justify enabling this for the default path.
No samples are discarded. These results include lifecycle cost and do not prove
which part of startup/scheduling/TLS causes the penalty. A future mechanism or
long-text-specific policy needs distinct evidence; do not repeat this prototype
unchanged or present it as DB acceleration. Source/gate/binary/compiler hashes,
raw vectors' exact-bit validation flags and every process output retained.
