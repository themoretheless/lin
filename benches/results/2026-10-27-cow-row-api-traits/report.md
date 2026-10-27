# COW Row Eq/Hash API regression audit

The production BTreeMap Row alias implements Eq and Hash because Cell does. The frozen COW newtype omitted those derives. A downstream-style public API probe succeeds on the production baseline, fails to compile on the frozen COW source with Eq/Hash trait-bound errors, and succeeds after adding only Eq and Hash derives to Row. Raw logs, statuses, exact source hashes and the corrected candidate-store.rs are retained.

The fixed probe verifies Eq/Hash/Send/Sync/UnwindSafe/RefUnwindSafe bounds; the same-process DefaultHasher digest matches an equivalent BTreeMap; a HashSet-held clone remains unchanged when the separate row mutates; JSON and MessagePack bytes match the map. The payload covers UTF-8 long text, a float and null. This is not a stable cross-Rust-version hash specification or broad full-feature runtime qualification.

The correction is a follow-up prototype only. Production sources are restored, and Linux run40431125318 still uses the unchanged frozen COW source. Do not attach that run's future timing result to this amended binary without final qualification. This restores two missing traits but does not remove the declared newtype source-compatibility break: implicit exact BTreeMap assignments still require an explicit conversion. No complete backwards-compatibility or production-rollout claim is made.
