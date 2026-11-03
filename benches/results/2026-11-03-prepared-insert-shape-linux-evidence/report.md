# Prepared INSERT shape cache — rejected

Run https://github.com/themoretheless/lin/actions/runs/41609632878 at 43a0f0c2809ac9b3a2ee8b9f547ef1b7ceb2eff5 completed all 216 processes: 6156 operation samples, 108 whole-worker wall observations and 108 peak-RSS observations. Independent auditor verified runtime/source/harness hashes, ordinary x86_64 ELF workers, all exact result digests, workspace tests and every numeric window. This is a numerical rejection, not infrastructure failure.

Native 100k improved 28.1376% and 27.5883%, both six positive pairs and above A/A 4.6400%/4.4951%. Native 1k improved 2.9855%/2.7844% but only five of six positive pairs in each series. Native 10k was -2.1480% (zero positive pairs) and +0.6844% (four positive pairs; below A/A). Thus only two of six target windows passed.

Read controls passed eleven of twelve windows; join_filter 100k series1 was -5.5513% versus A/A 1.4609%. Resource controls passed eleven of twelve: whole-worker wall 1k series2 was -3.3160% versus tolerance 2.5865%. All RSS windows passed. Identical unrelated read-control losses do not establish causal regression, but the frozen rule does not permit discarding them.

Reject the current candidate; restore src/exec.rs to frozen baseline. Preserve the large-case win for future research rather than claiming all-size qualification. No unchanged numerical retry. The prepared local FULL WAL binary was built but never timed: host preflight refused before creating output. FULL WAL experiment is closed without numerical qualification for this rejected source; exact binary/build log are archived here. No runtime changes are merged to main. The complete eight-peer objective remains unfinished.

raw.tar.gz contains every downloaded artifact member plus the local ordinary FULL WAL candidate and build log. Every archive member was reread and verified against raw-manifest.json. Source prototype remains at d8871eb36b119b9960989bf4a5feea9877168cc5.
