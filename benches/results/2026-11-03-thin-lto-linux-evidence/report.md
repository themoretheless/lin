# ThinLTO release profile: rejected

Run https://github.com/themoretheless/lin/actions/runs/41628553530 at a738704710cf0fe1a40a8ab839429aca0c709edb completed 216 processes, 6156 operation samples, 108 whole-worker wall and 108 peak-RSS observations. Independent audit verified source/harness/helper hashes, both ELF binaries, actual rustc profile flags, exact result values/digests and every decision. Linux release workspace: 414 passed, zero failed, 3 ignored. This is numerical rejection, not an infrastructure failure.

| Native rows | Series | Median gain % | A/A % | Positive pairs | Passed |
|---|---|---:|---:|---|---|
| 1000 | 1 | 5.148832 | 2.975773 | 5/6 | False |
| 1000 | 2 | 0.105958 | 1.869054 | 3/6 | False |
| 10000 | 1 | 3.245614 | 0.811926 | 6/6 | True |
| 10000 | 2 | 3.572282 | 1.500983 | 6/6 | True |
| 100000 | 1 | 2.937140 | 1.667607 | 5/6 | False |
| 100000 | 2 | 2.815230 | 4.109500 | 3/6 | False |

Only 2/6 native target windows passed (both 10k). All 12 read-control windows and all 12 lifecycle/RSS windows passed. Point-get medians improved 22.70–24.15%; join-filter 2.33–11.21%. These component benefits do not override the frozen all-target acceptance rule.

Candidate ELF: 5,258,328 bytes; baseline: 6,598,512 bytes. Clean/build elapsed: 90.413s versus 46.858s, respectively; these sequential builds are descriptive costs, not randomized compile-time qualification. Whole workspace correctness stage took 30 minutes on this runner.

Restore Cargo.toml to b9f10bc776b05eb5235237e9a900c2ef6aaed1fd. Prototype remains at 6c943cd0c54db4d35116f12559d800fcf3807334. All downloaded files, both ELF binaries and the complete workspace log are archived; every archive member was reread and SHA-256 verified. No unchanged numerical retry or main merge. FULL WAL and fresh peer qualification were not run for this profile. The eight-peer objective remains unfinished.
