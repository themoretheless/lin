# COW Row: next API version

The user approved development of COW with a distinct public Row type for the next API version on2026-10-28. This permits the source compatibility break; it does not waive correctness/performance qualification. Current production main remains separate.

This branch ports the lazy-sharing candidate by three-way merge from production04749994 onto6812cc5ca, preserving transaction atomicity/generation/abort, stale-HEAD recovery and schema-WAL rejection fixes. Direct MessagePack experiments remain separate. The port also retains the prototype in-place UPDATE, field-level undo and bounded index bitmap-prefix changes; it is not a representation-only experiment. These mechanisms must be qualified together before adoption.

Fresh/default/deserialized rows own an inline BTreeMap. Touched UPDATE rows are promoted to shared Arc storage; clones of shared rows reuse that map. Mutation detaches with Arc::make_mut, preserving held results/snapshots. Inline clones still deep-copy. Owning conversions, iterators, Eq/Hash and transparent map serde are retained.

Migration: replace implicit BTreeMap-to-Row assignments with map.into(); use Row::from, collect::<Row>(), or Row::new(). Convert back with BTreeMap::from(row). Map-style reads/writes remain available through Deref/DerefMut. Type identity has changed. Logical map content is stable; allocation, descriptor size and quota estimates require fresh qualification.

Acceptance still requires full correctness/API/lifetime tests, error/rollback/alias isolation, memory lifecycle/quota checks and fresh paired native/durable/read matrices with unchanged-peer controls. Previous eager COW had failed controls; the previous lazy Linux run was incomplete. No old timing result qualifies this port.

Local qualification of this port:443 default debug library/integration tests passed,2ignored. This includes the retained prototype model/isolation/ownership oracles and current transaction/crash regressions. These results establish bounded correctness coverage; no fresh performance result is claimed.
