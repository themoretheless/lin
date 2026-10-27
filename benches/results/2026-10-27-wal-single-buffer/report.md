# Single-buffer canonical MessagePack WAL: functional prototype

Hypothesis: avoid copying the encoded codec-1 payload into thread-local frame scratch by reserving nine envelope bytes, serializing the canonical payload directly after them, filling the header, appending the existing CRC32C, and issuing the same write/sync policy. This is independent of positive-zero compaction; payload bytes remain canonical and there is no new codec or eligibility scan.

Only the private Store durable-commit path uses the new scratch-buffer entry point. Public append_record and its existing buffer behavior are unchanged. Specialized InsertCols/AppendFactsBulk routes delegate to existing writers. The new private scratch contains the framed record; callers do not read it as a logical payload. Original core files are restored in finally after qualification. Candidate sources are retained, not installed in production.

Full default release library/integration suite:433 passed,0failed,2ignored (reported test lines including subprocess output). New oracle compares exact complete frames against the original write_frame, then canonical records through the existing decoder. Vector sizes0,1,768,14000,220000 cover empty/small vectors,64KiB reserve boundary and the old1MiB vectored-write boundary; values include -0, NaN payload, infinities and denormal. Both Normal and Full modes pass. An over16MiB record preserves existing file prefix and log-byte counter.

This is functional qualification only. Removing a copy does not establish a latency win or eliminate durable-sync cost. Next measurement must include complete encode+CRC+write+sync, baseline/control roles, small/scalar controls and both sync modes before a whole-engine gate. No existing peer victory is attributed to this prototype.

## Standalone local I/O screening: no stable target improvement

Twelve fresh release test processes, two six-process series; four record shapes, Normal/Full, baseline/control/candidate roles and12rotated observations each:3456operations. Each observation uses a fresh owned file with a4096-byte prefix written and durability-synced outside the timer. Output Vec capacity is reserved outside timing; the legacy TLS scratch is warmed by reference generation. Timing includes encode,CRC,envelope,write and mode-specific sync. File close/readback/removal are outside timing. All frame bytes and prior prefix match the reference.

This is a same-executable component screening on macOS, not separate engine source binaries or Linux Full qualification. Comparison uses per-process medians then paired percent medians. The analysis applies the existing six-positive/above-A-A target criterion to vector768; scalar and larger vectors are controls. This exploratory run is not a preregistered whole-engine acceptance run. No observation is removed.

| Case | Sync | Series | Gain % | Absolute A/A % | Positive | Pass |
|---|---|---|---:|---:|---:|---|
| scalar | normal | series1 | -1.537 | 10.571 | 2/6 | True |
| scalar | normal | series2 | -1.022 | 2.199 | 2/6 | True |
| scalar | full | series1 | -0.851 | 4.085 | 3/6 | True |
| scalar | full | series2 | 2.775 | 3.296 | 4/6 | True |
| vector768 | normal | series1 | 3.306 | 4.373 | 4/6 | False |
| vector768 | normal | series2 | -2.267 | 8.313 | 2/6 | False |
| vector768 | full | series1 | -1.129 | 6.690 | 2/6 | False |
| vector768 | full | series2 | -3.142 | 3.632 | 2/6 | False |
| vector14000 | normal | series1 | 0.602 | 1.985 | 4/6 | True |
| vector14000 | normal | series2 | 3.120 | 3.383 | 4/6 | True |
| vector14000 | full | series1 | -1.218 | 4.682 | 2/6 | True |
| vector14000 | full | series2 | 0.771 | 4.383 | 4/6 | True |
| vector220000 | normal | series1 | 0.135 | 0.752 | 3/6 | True |
| vector220000 | normal | series2 | -0.064 | 1.740 | 3/6 | True |
| vector220000 | full | series1 | 3.694 | 2.293 | 5/6 | True |
| vector220000 | full | series2 | 2.296 | 1.264 | 5/6 | True |

All four default-vector target windows fail. Some larger-frame medians improve, but they do not establish the requested ordinary durable-operation win. Component gate=false; no production adoption or full-engine run for this revision. `analyze-io.py` independently checks all raw log/JSON coordinates, immutable core/helper/oracle provenance, complete sample counts and recomputes every pair. Initial harness JSON-macro compilation failed before timings; that log/source/provenance remain separately named. Final source restoration is verified.
