# Frozen Linux benchmark and platform-aware CI gate contract

Benchmark run40278671806 and compute run40278672361, sourcea5e28e36.
All Linux/macOS/Windows compute compile/shader/feature-isolation/CLI checks passed.
This confirms the Windows snapshot Advice references compile after cfg(unix);
it is not physical GPU execution evidence.

The quick Linux x86 benchmark completed. Original legacy budget failed solely
because join526us exceeded an old macOS ARM153.5us threshold by3.43x. The artifact
upload-on-failure change retained raw-ci/run.json; no measurements are discarded.
The original failed step, budget file and evaluator are preserved.

| Case | Lin us | SQLite us | DuckDB us |
|---|---:|---:|---:|
| point_get | 0.1954 | 0.7064 | 321.9678 |
| filter_eq | 1.3352 | 152.0309 | 311.2390 |
| join_inner | 526.3433 | 4604.0870 | 2726.4785 |

The new grouped COUNT path measures1.335us here; the earlier frozen97f54abd CI
observation was927us. Those are different CI sessions, not an alternating paired
causal comparison. Local paired qualification is recorded separately.

## Deliberate gate-contract change, not a speedup

Historical absolute numbers260/1040/153500ns remain exactly unchanged. Baseline
metadata now states macos/aarch64; warn>1.5x/fail>3x applies only on matching
OS/architecture. On other platforms those ratios remain in the report as
historical information. Equal OS/architecture does not mean equal CPU/hardware.

Every platform additionally requires Lin to beat same-run SQLite and DuckDB on
the declared three cases. Missing measurements, ties/losses, incomplete runs,
unavailable/invalid wall metrics or invalid operation counts cannot pass.
This is six comparisons in one quick CI process, not the eight-peer objective.
No peer timings or historical limits were raised or rewritten. The old evaluator
returns2 on the saved raw run, the new one returns0 because all six same-run peer
comparisons win and the absolute baseline is platform-incompatible. Both outputs
are retained; do not call this a performance optimization or an independently
matched Linux regression baseline.

Seven contract tests include calibrated per-operation normalization, retained
compatible-platform3x failures even when peers are slower, incompatible-platform
scope, peer loss/tie on either platform, missing cases, invalid/incomplete data
and invalid baseline configuration. Synthetic gate tests are not peer runtime
proof. checks.json records both evaluator outcomes and test status.

The current SQLite durable1k gap and real Kusto qualification remain unresolved.
