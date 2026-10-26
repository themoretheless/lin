# Single reverse-map write prototype

Main205 index.rs, separate from all rejected FTS/row-undo candidates. insert_key previously inserted (idx,key.clone()) to obtain the old key, then wrote the same reverse mapping again in both forward-entry branches. Candidate removes the old reverse key first and writes the new mapping only in the final branch. Unique check and unchanged-key early return remain before modification. Forward posting removal, bitmap selection and partial predicate behavior remain unchanged.

274 tests pass, 2 existing ignored, including unique partial membership exit, failed transaction rollback, WAL replay/checkpoint and mutation accounting contracts. Initial command named a nonexistent index_composite test target; failure log preserved, command corrected to actual semantics_tests target. Production source restored exactly.

No allocation or latency measurement yet. Fewer source operations are a hypothesis, not evidence of faster execution. Full engine native/durable/read and peer gates are still required. Candidate is not production-enabled.
