# COW Row Linux qualification: positive medians, stability gate failed

Run40421852372,frozen headf9def96dac9c7fe4af364bbef240bea63e8fe553. Two six-round A/A/B series,36completed measurement processes,20cases,5760rawobservations. Independently recomputed240casecomparisons via verify.py. Tests and clone-loss sensitivity ran before build. Qualification false,gateexit3; full objective false.

| UPDATE | Series1 gain | A/A | Positive | Series2 gain | A/A | Positive |
|---|---:|---:|---:|---:|---:|---:|
| 1k | +22.400% | 10.625% | 6/6 | +24.024% | 17.723% | 4/6 |
| 10k | +10.358% | 3.969% | 6/6 | +8.853% | 6.903% | 5/6 |
| 100k | +6.654% | 5.481% | 5/6 | +5.713% | 3.152% | 5/6 |

All target medians positive and above median A/A, but required6/6positivepairs missing in four of six target/series combinations. Full control table and all losses in paired-report.md. Some controls have large A/A (SQLite DELETE10k series2 45.475%), so no unique causal attribution is established. Candidate beats SQLite in0/120Lin scenarios; even observed medians do not close the peer gap.

Do not call this qualified,round away failures,accept warm CPU48% as engine proof,or silently narrow to1kseries1. Prototype remains experimental,production source main205 unchanged. Public Row changes from BTreeMap alias to newtype; external exact-type API compatibility remains unresolved despite406defaulttests and all-targets compilation passing separately. Native/durable insertion,read/point/materialization,RSS,cold and broader peer qualification remain open.

Further work needs identified resource/variance causes or a distinct implementation/protocol hypothesis,not unchanged reruns seeking green. Retain this full negative/stability record if a subsequent environment/protocol qualification is performed.
