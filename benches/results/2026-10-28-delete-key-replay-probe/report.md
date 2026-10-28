# DELETE replay-key representation diagnostic

No production code changes. Eight standalone fixture cases encode the existing canonical Pack::Delete format twice: full selectors and identity projection. The three selectors reference two distinct rows (one repeated selector). Every prefix retains full rows with768float embeddings. Both WAL streams replay through the current reader; serialized surviving rows/IEEE bits,generation,next_id,CRC rejection and selected docs lookup/SoA observations match.

|Case|Full DELETE frame bytes|Projected frame bytes|Survivors|
|---|---:|---:|---:|
|id_uri|12200|215|6|
|uri_only|12131|146|6|
|stale_id_uri_fallback|12218|233|6|
|facts_tuple|12258|273|6|
|facts_missing_part|12198|213|6|
|custom_id|12202|217|6|
|custom_no_identity|12058|12058|8|
|nested_batch|12238|253|6|

Projection retains both id and uri when present,plus s/p/o for facts. URI fallback is explicitly tested with a stale id. Arbitrary non-facts rows without text id/uri retain the full original selector; that control keeps identical frame length and deletes nothing. Nested Batch is covered. No new WAL opcode is used.

This is a bounded current-reader replay diagnostic,not DB DELETE or a production codec. It does not verify old-reader compatibility,output/rollback ownership,index/FTS semantics across a real transaction,quota/canonical16MiB rejection behavior,crash/power-loss recovery or performance. Those are required before any candidate integration. No byte reduction is reported as latency improvement. Original unmodified production source hashes and compiled probe hash are recorded;all temporary example sources were removed.

Complete encoded streams and expected surviving-row bytes are preserved in fixtures.tar.gz with every archive member verified. Source reference remains the current frozen COW+CAS plus separate single-Cmp experiment;that index change is unrelated to the replay representation.

Legacy reader qualification for these fixtures: the complete production source snapshot04749994b9ca7a090476b95401f0f7166d8b870a was compiled in an isolated temporary directory with exact Git file hashes. Its Db::apply_wal accepted both streams for all8cases, verified generation2/next_id77, matched canonical surviving-row bits and made repeated application a no-op. Both CRC corruption and a modified expected surviving state were rejected at runtime;all fixture bytes were restored and the8positive cases passed again. Current sources and legacy library sources remained unchanged. This establishes only bounded old-reader compatibility,not production DELETE/rollback/quota/error-atomicity or performance acceptance.
