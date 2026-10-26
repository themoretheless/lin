# Windows snapshot mmap build fix

Compute CI40276421093 on frozenf19f41a4 failed on Windows because persist.rs
called `Mmap::advise(memmap2::Advice::Sequential)` twice without cfg(unix).
The actual memmap2 version exports Advice and advise only on Unix, as shown by
the saved compiler errors. Existing cold.rs already uses the correct guard.

Add the same cfg(unix) to both snapshot/backup advice calls. File mapping,
decode, checksum and errors stay unchanged; Unix still issues the same optional
sequential hint. This fixes the observed unavailable API references, not a
performance claim. Local macOS GPU/CPU compile and snapshot tests are recorded
in checks.json. A new Windows CI run is required for complete platform proof.
