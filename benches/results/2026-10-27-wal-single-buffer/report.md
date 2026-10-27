# Single-buffer canonical MessagePack WAL: functional prototype

Hypothesis: avoid copying the encoded codec-1 payload into thread-local frame scratch by reserving nine envelope bytes, serializing the canonical payload directly after them, filling the header, appending the existing CRC32C, and issuing the same write/sync policy. This is independent of positive-zero compaction; payload bytes remain canonical and there is no new codec or eligibility scan.

Only the private Store durable-commit path uses the new scratch-buffer entry point. Public append_record and its existing buffer behavior are unchanged. Specialized InsertCols/AppendFactsBulk routes delegate to existing writers. The new private scratch contains the framed record; callers do not read it as a logical payload. Original core files are restored in finally after qualification. Candidate sources are retained, not installed in production.

Full default release library/integration suite:433 passed,0failed,2ignored (reported test lines including subprocess output). New oracle compares exact complete frames against the original write_frame, then canonical records through the existing decoder. Vector sizes0,1,768,14000,220000 cover empty/small vectors,64KiB reserve boundary and the old1MiB vectored-write boundary; values include -0, NaN payload, infinities and denormal. Both Normal and Full modes pass. An over16MiB record preserves existing file prefix and log-byte counter.

This is functional qualification only. Removing a copy does not establish a latency win or eliminate durable-sync cost. Next measurement must include complete encode+CRC+write+sync, baseline/control roles, small/scalar controls and both sync modes before a whole-engine gate. No existing peer victory is attributed to this prototype.
