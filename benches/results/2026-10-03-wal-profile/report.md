# Diagnostic WAL phase profile

Temporary instrumentation on baseline cc5008b separates column encoding,
checksum/header construction, four write_all calls, and durable_sync. Native
macOS ARM host; Full sync preserved. Three independent processes, 24 fresh
one-operation samples per size. Each phase includes 27 events per process
(the 24 observations plus harness validation/preflight calls). Medians within
each process, then median of process medians. No builds/tests overlap timing.

| Documents | Payload bytes | Encode ms | Checksum ms | Write ms | Sync ms |
|---|---:|---:|---:|---:|---:|
| 1,000 | 336,968 | 0.532375 | 0.011667 | 0.045875 | 0.882709 |
| 10,000 | 3,558,340 | 5.662250 | 0.114958 | 0.273541 | 1.341334 |

Diagnostic measurements include clock reads and stderr trace logging between
stages. They are not clean before/after performance proof or engine comparison.
Separate phase medians must not be added to estimate a median total. Host load,
filesystem scheduling and cache state are uncontrolled. The measured write
stage is much smaller than encode/sync; batching syscalls is unlikely to close
the SQLite gap by itself. Sparse vector encoding scales substantially with size
and should be the next profiling/optimization target. Durability is unchanged.

The instrumented benchmark builds and all three runs are complete with the
existing fresh-fixture affected-count/readback gates. Production persist.rs was
restored byte-for-byte before running the pinned diagnostic executable; no
profiling code is retained in production. Raw traces, run.json, source copies,
and binary/source hashes are retained. The full eight-engine goal is unproven.
