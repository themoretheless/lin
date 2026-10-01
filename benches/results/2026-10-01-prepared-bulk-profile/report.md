# Prepared bulk sampling diagnostic

`cargo run --release --offline --example profile_bulk` repeatedly inserts 10000
prepared rows into a fresh memory Db with the scalar index. The loop includes
schema setup, affected-count/row-count assertions and destruction; it is not the
native benchmark timer. Preparation/source creation are before PROFILE_READY.
A five-second macOS sample (1 ms interval) captured 3786 main-thread stacks.
The complete 20.004-second run inserted 1192 batches and passed count checks.

Top-of-stack observations include memcmp 572, allocator free 401, tiny allocation
275, memmove 265, HashingEmbedder::fill 127, Scratch::bump 115, Scratch::finish 103,
Unicode character iterator 81, FtsIndex::append_row 79, lowercased 63 and Unicode
lower conversion 63. These are sample counts, not exact CPU/elapsed percentages.
FNV/content hashing can be inlined; absence of a named hot frame does not prove
its exact cost. Unicode conversion motivated the independently tested ASCII
path; this profile does not itself demonstrate a speedup or peer win.

Raw stacks are in sample.txt; the workload is examples/profile_bulk.rs.
