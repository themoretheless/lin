# Inline row keys: unqualified vnext prototype

Runtime base: 625bc7180b37027d661afb87009464617c34de36. This branch deliberately excludes the rejected FTS field-impact candidate. No adoption or performance claim.

Row stores field names as RowKey: UTF-8 names up to 22 bytes inline, longer names in Arc<str>. Cloning a bulk row template therefore avoids independent String allocations for field names. Comparison and hashing use the string contents; JSON and MessagePack encode ordinary string keys. Native 64-bit RowKey size is checked against String. Logical quota accounting must remain unchanged.

## Public API migration for the next version

Borrowed row iteration yields (&RowKey, &Cell). Use key.as_str() for borrowing and key.to_string() for owned String conversion. Deref now exposes BTreeMap<RowKey, Cell>; callers requiring BTreeMap<String, Cell> can consume Row into that map. Owned Row iteration still yields (String, Cell), and into_keys yields String. Concrete standard-library iterator types change; DoubleEndedIterator, ExactSizeIterator and FusedIterator are retained and tested. Existing insert(String, Cell) remains available.

## Validation record

Three focused release tests passed: UTF-8 boundaries and shared long keys; mixed-key ordering/hash/exact wire-byte parity; COW held-value isolation and quota parity. The first full release build failed in cow_row_owning.rs because it required concrete String-key iterator types and cloned borrowed keys as String. The follow-up preserves the iterator traits, item types and all runtime ownership assertions while adapting the concrete-type assumptions. Its result is recorded in test-release-iterators.log.

Remaining: complete full release validation, measure allocations, compare immutable baseline/candidate binaries with unchanged timed workloads and balanced process order, and run the full regression gates before adoption. This prototype does not resolve the SQLite FULL WAL gap or prove superiority over all peers.

Full release follow-up completed successfully: 461 passed, 0 failed, 2 ignored. This validates correctness coverage only; allocation and performance measurements remain pending.

Allocation mechanism test (tests/row_key_allocations.rs) passed in release: a 12-field template clone allocates 15 times with BTreeMap<String, Cell>, versus 3 with Row, eliminating exactly 12 field-name allocations. Both short and long field names were checked three times; equivalent values were asserted. This is not a timing result. See allocations-release.log.

Local timing plan uses the unchanged 12-case harness, six balanced role-order rounds, baseline/control/candidate, eight observations per process and 24 smoke processes. The DuckDB control uses the exact same baseline executable for all labels. The frozen source hashes and build inputs are recorded before measurement. This is local discovery, not Linux or full-suite performance acceptance.

The initial benchmark build failed because durable_json_row cloned a borrowed key into a serde_json::Map<String, Value>. Its build log and metadata are retained in attempt1-build. The candidate harness changes only that expression from k.clone() to k.to_string(), preserving the owned String JSON boundary. The original and adapted harness hashes are recorded separately; this is an explicit API migration, not byte-identical harness compilation.
