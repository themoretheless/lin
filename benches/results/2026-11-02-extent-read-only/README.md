# Read-only experimental extent database

Adds explicit `Db::open_extent_read(path)` behind experimental-extent-wal. The existing directory shared FENCE protects snapshot loading and log replay from checkpoint/reset. WAL is opened with read-only permissions, does not acquire the writer's exclusive file lock, and uses strict extent header/length/CRC/frame validation. The returned ReadDb retains the FENCE and its own stable in-memory snapshot. Ordinary legacy open_read remains its original format route.

Eight integration tests passed before the final portability cleanup. New checks cover read-only opening beside a live writer, stable old reader versus a fresh reader after a commit, rejected mutations with unchanged log/head bytes, corruption rejection without file rewrite, and a separate reader process holding the checkpoint fence. The writer can commit while that reader is alive; after reader process exit the exclusive fence and actual checkpoint succeed.

The test uses canonical persist::head_path (macOS case folding would hide a literal HEAD/head mismatch on Linux). Fence interoperability is tested with the same libc flock operation as the production FENCE, rather than relying on another lock API's backend. A full feature regression is running after those refinements.

No benchmark results or power-loss claims. Read replay currently materializes and validates the logical frame region more than once; streaming/open-latency optimization remains pending. Extent growth, standalone path-based export/bootstrap, directory/power-loss qualification and unchanged full performance acceptance remain open. Linux run 41391257035 stays frozen at the earlier 20a408f6c source and excludes this later change.

Final full feature regression passed: 480 tests, zero failures, four ignored helpers. The Linux-safe head_path and exact flock fence checks passed. Default-mode regression is still required after the shared read-open refactor.
