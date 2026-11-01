# Interpretation and next experiment

All 16 ordinary and 16 traced Linux probes completed with full-value validation after reopening. The independent audit reconstructed every mutation scope from original trace bytes. Original run: https://github.com/themoretheless/lin/actions/runs/41338904938 ; frozen commit 1bafaf7c388ddc90808db103cb59b37cfa49f9f4.

At both collection sizes Lin UPDATE/DELETE/COMMIT issue exactly one 4125-byte write followed by fdatasync. SQLite UPDATE/COMMIT issue six pwrite64 calls totaling 12360 bytes; DELETE issues eight totaling 16480 bytes. Each SQLite durable operation issues one fsync. No extra Lin file opens, checkpoints or sync calls occur inside these mutation scopes. Both rollback paths issue no data writes or sync calls; SQLite additionally takes/releases file locks.

These are syscall counts, not latency measurements. They rule out extra Lin mutation-scope syscall count or larger total write bytes as simple descriptions of this fixture; they do not establish a causal explanation for the earlier latency gap. File history, filesystem behavior, dirty metadata and scheduling remain uncontrolled here. Binary payloads were not uploaded; source and compiled binary hashes are retained in provenance, but this local audit does not rehash a Linux binary payload.

One concrete remaining difference is fdatasync versus fsync. A separate frozen experiment can replace only the Linux Full sync primitive with sync_all, preserving the sync barrier, recovery, quota, CRC and output contracts. It must be measured against the unchanged runtime with A/A controls; there is no assumed latency benefit. This cannot improve rollback directly, because rollback performs neither syscall. Keep all previous rejected performance candidates rejected.
