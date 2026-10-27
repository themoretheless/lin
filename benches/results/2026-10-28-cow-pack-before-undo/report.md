# COW Pack-before-undo hypothesis

Runtime owner diagnostic established that pending Packs retain updated Row storage during rollback. This candidate clears those uncommitted Packs before explicit rollback/Drop undo, then takes the stored row and applies its RowDelta without creating an extra shared-map owner. Before/after identity comparison uses cloned Cells rather than an allocated temporary map. Existing index/FTS removal and reinsertion retained, restored row installed before identity/cache rebuild. Client-held Handles still force COW detachment.

Full debug suite444passed/2ignored. Held UPDATE result regression now exercises explicit rollback and Drop, complete original-row/generation restoration, held-result equality and later independent client mutation. Freeing Packs remains inside the complete transaction rollback lifetime/timer. Durability, record content and public vNext Row semantics are unchanged.

This is a new combined ownership hypothesis after the failed705dbd035 experiment; the old losses remain retained. No latency claim before fresh A/A/B measurement and broader native/durable/read/memory qualification.
