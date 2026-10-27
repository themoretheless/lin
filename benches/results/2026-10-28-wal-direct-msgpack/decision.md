# Direct MessagePack mutation encoder: qualification in progress

The CPU experiment passed its target and control gates in two six-pair series (12 fresh processes). UPDATE/DELETE encoding improved approximately 72–73%; these are serialization measurements, not commit latency. The independent CPU verifier passed.

Full release library/integration validation: 437 passed, 0 failed, 2 ignored. The test harness restored all production source hashes after execution. The initial target-directory error occurred before compilation and is preserved separately.

Not accepted for production yet: runtime negative-zero mutation sensitivity and separate legacy-reader compatibility now passed (see ../2026-10-28-wal-direct-legacy-reader/report.md); still requires full-engine source A/B qualification with unchanged controls and durability. The complete eight-peer objective remains unachieved.
