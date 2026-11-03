# Vector finish Linux diagnostic evidence

Run https://github.com/themoretheless/lin/actions/runs/41636139554 at 49fed3aecc8da709b286706123dd6cbf35879208. Independent audit verified six processes / 18 wall observations, reconstructed the temporary patch, checked complete runtime/harness/manifest hashes, exact ELF binaries, actual package compilation, six-field readback and all diagnostic intervals/row totals. All raw members and binaries archived and reread against SHA-256 manifest. Production source remains frozen b9f10bc776b05eb5235237e9a900c2ef6aaed1fd.

| Rows | Normalize ns/row | Arc allocation+copy ns/row | Cleanup ns/row |
|---:|---:|---:|---:|
| 1000 | 89.774 | 2587.886 | 59.240 |
| 10000 | 89.886 | 3313.488 | 60.722 |
| 100000 | 97.119 | 3796.859 | 62.332 |

Within this instrumented default fixture, the Arc allocation/copy interval dominates normalization and cleanup. It does not separate allocator bookkeeping, page faults, physical memory writes or scheduling delays. Per-thread durations overlap; sums are not wall-time fractions. Diagnostic Scratch adds counters, changes layout and executes three timers per row. Ordinary/diagnostic wall differences are not paired candidate qualification and cannot establish speedup. No normalization model, dimension, public Row/Cell or default behavior changed.

Next: investigate output-memory ownership/allocation strategies against preserved historical arena/fused-output failures before implementing a new candidate. Existing rejected candidates and frozen control rules remain binding. No new public/private peer or FULL WAL victory is claimed; the full eight-peer objective remains open.
