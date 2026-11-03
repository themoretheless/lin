# Prepared INSERT shape cache — prospective Linux qualification

Candidate runtime d8871eb36b119b9960989bf4a5feea9877168cc5 is compared with frozen b9f10bc776b05eb5235237e9a900c2ef6aaed1fd. The complete runtime closure differs only in src/exec.rs. Public Row remains BTreeMap; values and timestamps are resolved at execution. Default workspace local validation: 417 passed, zero failed, three ignored (candidate receipts are in the adjacent candidate directory). Performance is not yet qualified.

Two series of six paired rounds use all six baseline/control/candidate order permutations. Warm native INSERT targets cover 1k, 10k, and 100k rows; point_get and join_filter are frozen controls at all sizes. Each of six target windows requires six positive pairs and median paired gain greater than independent A/A variation. Each of twelve read control windows permits at most max(2%, A/A) regression.

The warm phase contains 108 processes, 324 case records and 6048 operation samples. A separate native-only phase contains 108 fresh processes with one operation each, exact six-field readback, and canonical digest validation. Process wall includes startup, preparation, result serialization and teardown; Linux os.wait4 records peak RSS in KiB. Twelve resource windows permit at most max(2%, A/A) wall regression or max(5%, A/A) RSS regression. All warm and resource gates must pass. The combined scope is 216 processes, 6156 operation samples, 108 lifecycle-wall observations and 108 peak-RSS observations.

The resource phase runs even after a numerical warm rejection; functional failures retain partial artifacts and stop. Unchanged numerical retries are forbidden. Source/harness/helper hashes, compiler, host CPU/affinity/cgroup limits and ordinary ELF binary hashes are retained. The native lifecycle helper has a non-shadowing filename and an actual subprocess/os.wait4 preflight.

A pass would qualify only this source/resource comparison. Fresh peer comparisons, mixed-layout fallback coverage beyond existing functional tests, durable FULL WAL, other platforms and the complete eight-peer objective remain open. This run does not establish peer superiority.
