# Shared reverse index keys: rejected overall;10k and memory signals retained

Run https://github.com/themoretheless/lin/actions/runs/41622380929 at0fe56ce58f1779d875f5508519fd0273723f7109 completed216 processes/6156operation samples/108lifecycle-wall/108RSS. Independent audit verified all runtime/harness/helper closures, actual package compilation for both ELF workers, workspace tests, exact readbacks/digests and all numerical windows. Local workspace415passed/0failed/3ignored. Numerical rejection, not infrastructure failure.

| Native rows | Series | Median gain % | A/A % | Positive pairs | Passed |
|---|---|---:|---:|---|---|
| 1000 | 1 | 1.814387 | 0.982684 | 5/6 | False |
| 1000 | 2 | 1.113012 | 0.284851 | 5/6 | False |
| 10000 | 1 | 4.383365 | 1.237160 | 6/6 | True |
| 10000 | 2 | 4.915842 | 1.097214 | 6/6 | True |
| 100000 | 1 | -0.755122 | 3.665934 | 3/6 | False |
| 100000 | 2 | -0.274976 | 5.214379 | 2/6 | False |

Both10k targets passed (+4.383365%/+4.915842%,6/6each,aboveA/A). Neither1k nor100k target passed:1k had5/6positive pairs each,100k medians were slightly negative. All twelve read controls passed; all twelve lifecycle/RSS controls passed. PeakRSS at100k improved1.854396%/1.841554%, above tiny A/A0.016662%/0.012049%;1k/10k median RSS difference was zero. These are bounded observed signals, not high-cardinality/unique-index qualification or universal memory claims.

Frozen all-size gate rejects current source. Restore src/index.rs to baseline b9f10bc776b05eb5235237e9a900c2ef6aaed1fd. Prototype remains at555a8fa16c88309618f2055045e67bc24c609f30. Every downloaded artifact/binary is archived and reread against manifest.json. No unchanged numerical retry, main merge, FULL WAL result or fresh peer superiority claim. Future research can use the preserved10k/memory signal only with a materially different hypothesis and new frozen qualification. Eight-peer objective remains unfinished.
