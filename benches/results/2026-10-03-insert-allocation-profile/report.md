# Diagnostic insert allocation counts

Dedicated single-thread Rust example with a temporary counting global allocator.
Three processes, three fresh fixtures per size per process; all nine counts
match exactly. Prepared source, schema and validation are outside counter scopes.
Same six common document fields plus wiki layer and default embedding/FTS;
constant timestamp and two wing/ts keys. Exact six-field readback and 768-dimension
embedding presence checked after each insert. Counters cover alloc, alloc_zeroed
and realloc calls; bytes are requested allocation/reallocation sizes, not live
or peak memory. Deallocation is not counted. The diagnostic allocator changes
runtime overhead/optimization, so no speed comparison is made from these runs.

| Stage | 1k calls | 1k requested bytes | 10k calls | 10k requested bytes |
|---|---:|---:|---:|---:|
| Row build/checks | 16,003 | 866,648 | 160,003 | 8,606,272 |
| Embedding | 3,010 | 3,188,160 | 30,011 | 31,865,116 |
| Scalar index | 31 | 414,617 | 42 | 3,441,585 |
| FTS | 2,060 | 264,881 | 20,081 | 2,543,785 |
| Row-map registration | 2 | 8 | 2 | 8 |

Row build includes record construction/default hash/uniqueness/FK checks and
initial built/check-set buffers. Row-map/index reserve calls between measured
stages, edges, row movement, final pack/Handle construction and subsequent drops
are excluded. Counts are stage-specific and not the entire insert allocation
budget. Record keys require owned Strings under the stable public Row alias;
text Cell construction creates Arc allocations for prepared String literals.
A distinct next candidate is shared AST text literals so value_cell can clone
existing Arc storage without a separate side cache, preserving stable Row/Cell
behavior. Earlier side-cache and sorted-row experiments were rejected and must
not be treated as performance proof for this candidate.

Build and all three diagnostic processes passed. Production exec.rs/lib.rs and
Cargo.toml restored byte-for-byte before running the pinned executable. Diagnostic
sources, hashes, raw logs and allocation ranges retained. No production allocator
or hooks retained. Full eight-engine objective is unproven.
