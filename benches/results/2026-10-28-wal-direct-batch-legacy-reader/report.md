# Direct Batch writer: legacy-reader compatibility

Separate producer/legacy reader processes passed five phases with complete canonical rows, exact generations and IEEE edge values. Reader persist.rs is byte-identical to production04749994b9ca7a090476b95401f0f7166d8b870a. Producer uses the exact direct-Batch candidate; hashes in provenance.json.

UPDATE Batch frame8099->2275bytes; DELETE4057->1149; mixed INSERT/UPDATE Batch8103->5187. Explicit rollback produced no frame or generation advance. Reader rejected corrupted frame and modified complete-row expectations at runtime. No production adoption, performance or power-loss claim.
