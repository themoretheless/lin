# First and repeated 100k INSERT on Linux

Profile of engine fe2717127 with a timed worker (cebc9c429). Eight independent worker processes per host each inserted 100k rows into five fresh in-memory databases. Each transaction executed ten prepared 10k statements. The worker recorded elapsed time around each statement and differences in process-wide minor fault counts from /proc/self/stat. Preparation and final commit are outside these intervals. x64 used static TCMalloc and ARM64 used the system allocator.

For each process, subtract the median of its four repeats from its first insertion. Each cell below gives the median across the eight processes. The total uses the sum of ten statement intervals for each insertion before aggregation. Linux minor faults include all worker threads. The measurements do not isolate the time spent handling faults.

## AMD EPYC 9V74 80-Core Processor (chunks-ubuntu-22.04-1)

| Chunk | Extra time (ms) | Extra minor faults | Processes slower on first insert |
|---|---:|---:|---:|
| All | 74.410 | 105027 | 8/8 |
| 1 | 7.216 | 11134.5 | 8/8 |
| 2 | 7.372 | 10700.5 | 8/8 |
| 3 | 8.437 | 11071.2 | 8/8 |
| 4 | 6.403 | 9689 | 8/8 |
| 5 | 8.249 | 11082.5 | 8/8 |
| 6 | 8.922 | 11707 | 8/8 |
| 7 | 6.140 | 9216 | 8/8 |
| 8 | 6.313 | 9916 | 8/8 |
| 9 | 10.149 | 12347 | 8/8 |
| 10 | 5.176 | 8323.5 | 8/8 |

## AMD EPYC 9V74 80-Core Processor (chunks-ubuntu-22.04-2)

| Chunk | Extra time (ms) | Extra minor faults | Processes slower on first insert |
|---|---:|---:|---:|
| All | 75.075 | 104912 | 8/8 |
| 1 | 7.535 | 11132 | 8/8 |
| 2 | 7.612 | 10702 | 8/8 |
| 3 | 8.229 | 11065.8 | 8/8 |
| 4 | 6.329 | 9689.5 | 8/8 |
| 5 | 8.298 | 11083 | 8/8 |
| 6 | 8.718 | 11705.5 | 8/8 |
| 7 | 6.018 | 9217 | 8/8 |
| 8 | 6.224 | 9913 | 8/8 |
| 9 | 10.426 | 12336.5 | 8/8 |
| 10 | 5.396 | 8322.5 | 8/8 |

## Neoverse-N2 (chunks-ubuntu-24.04-arm-1)

| Chunk | Extra time (ms) | Extra minor faults | Processes slower on first insert |
|---|---:|---:|---:|
| All | 62.041 | 95462 | 8/8 |
| 1 | 5.897 | 10030 | 8/8 |
| 2 | 6.216 | 9662 | 8/8 |
| 3 | 7.244 | 10193 | 8/8 |
| 4 | 5.613 | 9052 | 8/8 |
| 5 | 5.437 | 9364 | 8/8 |
| 6 | 8.453 | 10678 | 8/8 |
| 7 | 5.510 | 9400 | 8/8 |
| 8 | 5.633 | 9057 | 8/8 |
| 9 | 5.630 | 9728 | 8/8 |
| 10 | 5.469 | 8549.5 | 8/8 |

## Neoverse-N2 (chunks-ubuntu-24.04-arm-2)

| Chunk | Extra time (ms) | Extra minor faults | Processes slower on first insert |
|---|---:|---:|---:|
| All | 60.657 | 95462.2 | 8/8 |
| 1 | 5.388 | 10030 | 8/8 |
| 2 | 6.082 | 9662 | 8/8 |
| 3 | 7.262 | 10193 | 8/8 |
| 4 | 5.333 | 9052 | 8/8 |
| 5 | 5.329 | 9364 | 8/8 |
| 6 | 8.486 | 10678.5 | 8/8 |
| 7 | 5.617 | 9400 | 8/8 |
| 8 | 5.451 | 9057 | 8/8 |
| 9 | 5.345 | 9728 | 8/8 |
| 10 | 5.270 | 8549.5 | 8/8 |

[Workflow and raw artifacts](https://github.com/themoretheless/lin/actions/runs/45996769804)
