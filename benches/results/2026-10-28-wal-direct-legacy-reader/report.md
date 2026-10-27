# Direct mutation MessagePack: standalone legacy-reader compatibility

Separate producer and reader processes passed complete canonical-row and generation comparisons in five phases: seed, UPDATE Batch, DELETE, explicit rollback, and mixed INSERT/UPDATE Batch. The reader persist.rs is byte-identical to production commit 04749994b9ca7a090476b95401f0f7166d8b870a; exact producer/helper and binary hashes are in provenance.json.

DELETE frame: 4057 canonical bytes to 1149 bytes. Batch falls back to canonical encoding (8099 and 8103 bytes); it is not compacted by this candidate. IEEE edge values were preserved by the legacy decoder. Corrupted frame and changed complete-row expectations were both rejected by the reader at runtime.

The first adapted harness incorrectly required UPDATE Batch compaction from the previous encoder. Its failure, fixtures and provenance are retained under initial-batch-size-assumption. Only that size expectation was corrected; canonical-row/generation comparisons and the strict DELETE compaction assertion remain.

This proves compatibility for these fixtures, not full-engine performance or power-loss durability. No production source changes remain.
