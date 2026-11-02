# Fixed physical EOF: sync diagnostic, not a WAL candidate

Question: does avoiding file-length growth reduce point-write + fdatasync latency on the actual Linux runner? Earlier 4125-vs-4096 append probes did not establish a uniform alignment penalty, and switching to fsync did not resolve the gap. This tests a distinct physical-file-layout hypothesis.

One std-only Rust binary, payload 4125 bytes, 64 syncs per fresh file/process. Compare growing EOF, fixed sparse EOF, and fixed fully zero-written EOF. Each fixed mode also writes a 16-byte logical-end/complement header before the same one fdatasync; append mode retains one write. Setup and initial sync_all are outside measurement. Verify every payload and the final header after reopening, then remove only the create_new-owned file. Freeze source, executable hash, compiler and host identity.

Two labels per mode, six balanced Williams rounds, all labels run the same binary: 36 fresh processes, 2304 timed sync observations. Retain all raw data and per-mode same-binary control variation. No production source, WAL frame, CRC policy, recovery rule, durability contract or benchmark acceptance threshold changes.

This is not a crash/recovery proof, equivalent WAL implementation, or all-peer benchmark. In particular, the complement header is only readback instrumentation, not an admitted on-disk format. Any future physical format would require separate versioning, strict CRC/torn-write handling, crash tests and compatibility decisions before performance qualification or adoption.
