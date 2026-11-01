# Why grouped inserts take a different encoding path

Source inspected in the exact corrected runtime 625bc7180 (the fsync experiment changes only the Linux sync syscall and does not affect this dispatch).

- InteractiveTxn::commit in src/exec.rs calls fold_packs and passes the resulting pack into durable_commit.
- fold_packs returns one child unchanged, but wraps two or more children in Pack::Batch.
- append_record in src/persist.rs selects append_insert_cols_v2 only for a top-level Pack::InsertCols. A top-level Batch falls through to append_record_v1.
- append_record_v1 calls rmp_serde::encode::write_named and checks the resulting byte length against MAX_RECORD. This is the exact error reported by the failed 10k grouped-seed probe.
- append_insert_cols_v2 uses encode_insert_cols_v2 for a valid edges-free field/column layout; it preserves the existing record envelope, CRC and sync path.

This establishes the differing dispatch, not the precise byte expansion or latency impact. The attempted 10k grouped fixture failed before the target operation; no speed result exists.

A new candidate can specialize Batch only when every child is an edges-free InsertCols for the same collection with identical field order and compatible column types/lengths. It must preserve child order, every scalar/vector bit, row count, next_id, generation and canonical quota accounting. The existing typed-column format could represent the combined append without a new wire version, but real old-reader execution is still required before claiming compatibility. Mixed collections, schema mutations, incompatible columns, edges and nested packs must retain the original Batch path.

Prefer streaming columns across validated child packs over cloning all row payloads. Validate the entire candidate shape before writing any bytes. Retain the original Pack for quota and runtime semantics. Test all column types, empty/null values, vector bit patterns, incompatible fallbacks, limits, injected errors, transaction rollback, replay and compiled missing-child negatives. A separate frozen benchmark must establish whether this helps grouped inserts and avoids regressions in existing workloads. Previous direct-Batch-MessagePack and other rejected candidates stay rejected; this proposal reuses typed columns and has not yet been implemented or measured.
