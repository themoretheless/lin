# Experimental Db integration of fixed-extent WAL

Parent container/codec source: f0365e1cd. Explicit `Db::open_extent(path, capacity)` and `open_extent_follower` select the experiment under `experimental-extent-wal`. Creation requires a new log; existing legacy logs are refused. Reopen requires the explicit API. Normal sync is refused for extent commits. Ordinary Db opening remains the existing legacy route.

Commit and replay use existing native frame codecs inside the extent. Cached log quota/statistics count logical frame bytes. Checkpoint syncs the extent before publishing snapshot/head, then clears only its logical boundary, retaining allocation. Db WAL export and follower apply use the extent transport. Portable backup stores the logical checkpoint offset. No automatic migration.

Prepared encoder failures/panics retain their previous error semantics. Ready prepared-column payloads are currently re-encoded through the verified native codec instead of reusing the worker payload; this is a known performance limitation to resolve before qualification.

The initial integration check failed on missing SyncMode qualification (log retained). The first integration run passed legacy-file preservation but found a cloned-descriptor cursor bug in export: from_file read the header without seeking to zero. The reader now seeks explicitly. The capacity test initially used an immutable body field and never reached storage; it now uses title. All original failure logs are retained.

Follow-up: four integration tests passed, one child helper ignored. Coverage includes transaction commit/rollback, exact frame shipping into an extent follower and memory follower, checkpoint allocation reuse, portable backup, snapshot reopen, memory/WAL invariance after capacity failure and Normal refusal, legacy-log preservation, and recovery of acknowledged writes after child process exit without Db Drop/checkpoint.

The full feature run adds a 4096-row columnar insertion case, full per-row wire-byte equality after shipping, checkpoint/reopen, and ordinary Db refusal without altering the extent file. Results are pending in full-feature-tests.log.

Still open: dedicated read-only extent API and cross-process reader qualification; standalone path-based export/bootstrap interfaces; prepared-column reuse; extent growth; directory/power-loss qualification; final default-mode regression suite and Linux crash tests; matched database benchmarks and the complete all-peer goal. The standalone 69.5% I/O gain does not establish a Db gain. This branch is not adopted or merged into main.

The expanded integration suite exposed unsafe legacy replay of the initial LINEXT01 header: the legacy JSON-length fallback treated it as a truncated tail and accepted/truncated the file. The new experimental LINEXT02 envelope begins with a complete LIN06 frame prefix and reserved unsupported codec 127, making current legacy checksummed readers reject it before any truncation. Header layout/CRC cover the new envelope; the mid-header SIGKILL hook is adjusted to interrupt the logical boundary. Original prototype files are not auto-converted. The failed full-suite log is retained. Poisoned handles now also refuse export. Final regression results are pending.

Final feature regression after LINEXT02 protection passed: 476 tests, zero failures, four ignored (including two child helpers). All five Db integration tests, 4096-row columnar case, current legacy Db-open refusal with file preservation, and five SIGKILL stages passed. Default-mode full regression is running separately. No database timing qualification yet.

Default-mode full release regression also passed: 463 tests, zero failures, two ignored. Feature and default correctness suites are both green; no database performance qualification is claimed.
