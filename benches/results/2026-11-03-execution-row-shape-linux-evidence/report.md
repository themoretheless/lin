# Execution-local shared row shape: rejected

Run https://github.com/themoretheless/lin/actions/runs/41614722491 at 4ef9c0d41c95acd1211e44d4edadb0df92846384 completed all216 processes,6156 operation samples,108 lifecycle-wall and108 RSS observations. Independent audit verified exact runtime/harness/helper hashes, actual compilation of lin/worker for both ordinary x86_64 binaries, all exact readbacks/digests, all numeric windows and workspace correctness. This is numerical rejection, not infrastructure failure. Local workspace416 passed/0failed/3ignored.

| Native target | Series1 gain | Series2 gain | Positive pairs |
|---|---:|---:|---|
| 1k | -3.09919% | +0.22566% | 2/6 and3/6 |
| 10k | -2.62821% | -4.62762% | 1/6 and0/6 |
| 100k | +15.79392% | +22.89054% | 6/6 and6/6 |

Both100k windows beat independent A/A2.83107%/1.26020%. Four smaller target windows failed. Three read controls failed: join_filter1k series1 -2.91236% versus A/A1.11091%;1k series2 -2.91294% versus A/A0.54856%;100k series1 -5.33849% versus A/A2.76524%. Other nine read controls passed. All twelve whole-worker wall/RSS windows passed. Target gains cannot waive frozen control failures; identical unrelated read paths do not establish causal regression but cannot be discarded.

Reject this source and restore src/exec.rs to baseline b9f10bc776b05eb5235237e9a900c2ef6aaed1fd. Source prototype remains at3d7fc7f09; all binary/raw artifacts remain archived and were reread against manifest.json. No unchanged numerical retry, main merge or peer superiority claim. Large100k improvements remain useful research evidence but do not qualify all-size behavior. No FULL WAL benchmark ran for this source. All eight-peer objective remains unfinished.
