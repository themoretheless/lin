# Rejected inline FTS token storage

Six alternating process pairs, 24 fresh one-operation samples per case.
1k delete: paired median gain -0.74%, wins 3/6.
10k delete: paired median gain 7.69%, wins 5/6.
Rejected because no consistent improvement across sizes. FTS unit tests passed
11 cases via pinned direct binary; cargo launch was terminated while stalled
in dyld startup. No production change retained. Raw samples and binary hashes
are retained.
