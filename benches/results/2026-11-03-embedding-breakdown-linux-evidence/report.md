# Linux hashing-vector breakdown: diagnostic evidence

Run https://github.com/themoretheless/lin/actions/runs/41635273783 at 816e5c2e8f010f005cd08e43522b723d1a16d28b. Six fresh processes, 18 wall observations. Independent audit reconstructed the instrumentation patch from frozen Git source, verified all source/manifests/harness hashes, actual compilation and exact ELF files, all six-field row values, all diagnostic observations and derived aggregates. All archive members reread and SHA-256 checked. Production Rust source and manifests remain byte-identical to b9f10bc776b05eb5235237e9a900c2ef6aaed1fd.

| Rows | Lowercase ns/row | Feature accumulation ns/row | Finish ns/row | Diagnostic chunks |
|---:|---:|---:|---:|---:|
| 1000 | 65.745 | 335.805 | 4166.351 | 12 |
| 10000 | 78.910 | 372.928 | 4302.970 | 12 |
| 100000 | 71.124 | 389.153 | 4289.194 | 120 |

Finish includes normalization, dense Arc allocation/copy and touched-slot cleanup. It accounts for much more instrumented worker time than feature accumulation on this fixture. This does NOT isolate allocator cost, prove hashing is cheap on arbitrary texts, establish wall-time fractions, or prove any optimization. Per-worker durations overlap; three per-text timers alter overhead and code layout. Ordinary/diagnostic wall medians are retained in independent-audit.json but are not an A/B speedup comparison.

Next investigation: separate normalization, output Arc allocation/copy and cleanup within finish; consider production changes only after attribution and a new frozen end-to-end gate. Existing fused-output and sparse-output experiments remain rejected; do not repeat them unchanged. No peer victory or eight-peer goal completion is claimed.
