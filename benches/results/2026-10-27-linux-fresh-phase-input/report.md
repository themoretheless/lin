# Linux fresh-fixture phase diagnostic input

Instrument exact benches/compare.rs setup_lin_write and write_update_lin,filter only existing UPDATE1k/10k/100kLin cases. Baseline main205 and frozen COW source from f9def96d. Prepared call and Handle drop separately timed.22inclusive stages: positions,update,delete,fks,index_insert,fts_sync,row_maps_sync,store_delete,move_tail,index_remove,swap_columns,drop_keys,fts_move,index_move,quotas,program,prepared,before_clone,updated_clone,undo_capture,store_replace,Pack_row_clone. Nested spans must not be summed.

Three separate processes per variant; eight benchmark wall observations per case with fresh inputs,default checked validation unchanged. Phase logger observes all write_update_lin calls,including framework calibration/validation; their phase records are retained without pretending every record is a timed sample. Local one-sample smoke captured12records,not3. No records are dropped to obtain a preferred answer.

Instrumentation adds Instant/atomic overhead,changes code layout and records data inside bench closure. Thus every instrumented wall result is diagnostic only,not acceptance or a causal speed comparison. No SQLite/peer claim and no loosening of the failed COW gate. Dump occurs at main exit; output is outside stage timers. Source and exact instrumented/binary hashes retained. All touched sources,including bench,restored in finally.

Local baseline smoke succeeded with original readback on1k/10k/100k. Linux profile is a distinct diagnostic action,not an unchanged attempt to rerun acceptance green. Public API/read/native/durable/RSS qualification remains open.
