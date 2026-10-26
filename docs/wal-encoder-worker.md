# Shared WAL encoder worker

The implementation was qualified against frozen baseline `53ebe9b` and accepted
for the measured durable batch scenarios. Evidence, raw controls and remaining
limitations are recorded in
[the full-engine report](../benches/results/2026-10-26-wal-persistent-engine/report.md).
This does not establish the full eight-peer benchmark objective.

For a single ordinary durable insert, eligible finalized row batches can overlap
column packing/payload encoding with existing index/FTS/HNSW/row-map maintenance.
The initial policy is 4096..=16384 rows, no edges, and 2Mi..=16Mi embedding float
values. Explicit transactions, multi-statement programs, native stores and other
batch/vector sizes retain sequential encoding. These bounds are a dispatch
policy, not universal performance guarantees.

A weak registry shares a bounded encoder queue among participating durable
stores. Each store acquires a lease lazily; the last lease closes the channel and
joins the worker. Queue capacity is one; full/disconnected queues or unavailable
worker creation return the owned job for sequential execution. A retiring worker
may briefly coexist with a newly acquired worker while shutdown completes, but
has no remaining store leases or new jobs. There is no thread per idle database.

Rows move into `Arc<Vec<Row>>`. Maintenance borrows them while the worker builds
the original logical column pack and payload. The worker explicitly drops its
row-container Arc before replying. The caller recovers the Vec without copying
row maps; strings and embedding values are not deeply copied to transfer work.
WAL column packing retains its existing ownership behavior.

Encoding errors and panic payloads return to the original commit location, after
quota and index error precedence. A panic is caught in the encoder so the shared
worker can handle subsequent jobs; it is resumed on the caller at the commit
point. This preserves prior panic behavior and does not promise rollback after
an arbitrary panic. Ordinary error rollback and no-WAL-write cases have focused
coverage. Metadata mismatch falls back to encoding the actual commit record.

Only column packing/payload encoding runs on the worker. File ownership,
framing, checksum, generation validation, append, Full sync and checkpoint stay
on the caller's commit path. There is no new codec or persistent pending cache.
The existing buffer is transferred and reused after successful commit. Rejected
or failed operations can drop that buffer and require a later allocation.

Qualification separates CPU feasibility from full commit latency, native from
Full durable, and process-exit recovery from physical power-loss testing.
Optional-feature compilation is not GPU runtime qualification. See the raw
reports for the tested fixtures and resource limits.
