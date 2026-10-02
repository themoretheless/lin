# Native MongoDB insert comparison

Three independent processes per size, nine fresh samples per process. Same six
fields, timestamp, unique identity/URI and wing/ts index; exact readback after
every sample. Setup, preparation, readback and cleanup excluded from timing.

| Documents | Lin median ms | MongoDB median ms | Mongo / Lin |
|---|---:|---:|---:|
| 1,000 | 2.668 | 15.226 | 5.71x |
| 10,000 | 20.784 | 100.576 | 4.84x |

Both require-wins runs completed successfully. MongoDB 8.0.28 on native ARM Docker
with tmpfs, ordered PyMongo insert_many, w=1/j=false. Lin uses Db::empty and
includes default embedding/FTS. Successful API insertion only: bulk failure
atomicity and physical disk durability are not equivalent. This does not prove
wins against all eight target engines. Raw process reports are in rows-1000 and
rows-10000. verification.json records binary SHA256, unchanged sentinel data,
and no remaining benchmark databases. The owned Mongo container was stopped.
