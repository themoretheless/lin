# Apple WAL preallocation

Writable empty WAL files on Apple platforms request 1 MiB of physical space using best-effort `F_PREALLOCATE`. Large frames (at least 64 KiB) check current allocation before writing and request additional space rounded to MiB; each request is capped at incoming frame size plus 1 MiB. Allocation failure leaves the ordinary write path in place. Other platforms use the existing path.

Logical file length, WAL encoding, checksum, Full/Normal sync behavior and logical quota accounting are unchanged. This policy reserves disk space while the writer is open; it is not an RSS allocation or physical quota guarantee. The cold 1k fixture used 1 MiB allocated versus roughly 340 KiB before this change. Unsupported-filesystem and ENOSPC fallback were not fault-injected.

[Real-engine qualification](../benches/results/2026-10-24-wal-preallocation-engine/report.md) contains both positive and negative results, source/binary hashes, calibrated pairs, cold resource observations and process-exit replay checks. The limited observed benefit is Apple durable 1k commit latency; SQLite remains faster in that scenario. Process-exit recovery is not power-cut qualification.
