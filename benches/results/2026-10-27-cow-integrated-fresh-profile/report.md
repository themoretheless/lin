# Integrated COW local fresh-fixture phase profile

macOS ARM64 diagnostic only, current main047 plus COW-inplace and Eq/Hash/take-all integration. No rejected selective-index or reverse-first source. Six successful processes,198phase records (including validation/calibration),144timed observations, three fresh samplesets per kind and size. All raw PHASE lines exactly match saved phase JSON. Instrumentation and wrapper bookkeeping affect wall timing; these figures are not peer or acceptance results. Inclusive stages must not be added together.

Median of process medians, nanoseconds:

|kind/rows|prepared|Handle Drop|index_insert|insert_key (inside index_insert)|row_maps|store_delete|move_tail (inside store_delete)|fts_move (inside move_tail)|
|---|---:|---:|---:|---:|---:|---:|---:|---:|
|update/1000|2875|42|417|250|417|0|0|0|
|update/10000|6375|41|1083|750|834|0|0|0|
|update/100000|10417|42|2583|2000|1291|0|0|0|
|delete/1000|3584|166|0|0|0|2500|1167|834|
|delete/10000|9208|417|0|0|0|6375|3250|2375|
|delete/100000|17000|750|0|0|0|12375|5916|4041|

Source correction: the actual benchmark catalog has no initial scalar indexes; setup creates only non-unique docs[wing,ts], with two initial keys verified by the fixture validator. Identifier/URI maps are separate. Selective skipping could not skip the patched wing index, and the unique-check reorder had no unique lookup to remove here. Their rejected warm measurements do not evaluate gains on hypothetical schemas with untouched or unique secondary indexes.

Dense postings already promote to bitmap at32members. PostingList::remove then calls count()==0 in index maintenance; BitSet::count scans all words. BitSet::is_empty already short-circuits at the first nonzero word. This is a concrete next source hypothesis, not a proven speed attribution. Dense, sparse and fully empty controls are required before retaining it. Linux diagnostic uses the same frozen source and scripts in a fresh output directory. An initial analyzer syntax error is retained; corrected analyzer verifies all existing raw records without rerunning measurements.
