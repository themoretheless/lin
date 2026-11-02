# Preserved Ubuntu 22 six-peer run: failed audit

Run: https://github.com/themoretheless/lin/actions/runs/41376860938

The original job failed at its audit assertion on source_drift. This archive retains all 84 downloaded artifact files byte-for-byte; raw-manifest.json records every member size and SHA-256, and archive.py verifies every compressed member after writing.

All 24 run.json reports were classified. Twelve have source_drift=true. Their recorded Git-status deltas add only untracked generated output directories beneath the protocol result directory, with no removed status entries. The other twelve have unchanged status. Original flags and data remain untouched.

This classification concerns the recorded status strings only. It is not a complete content-integrity audit, does not retroactively pass the failed CI job, and supplies no qualified performance conclusion. A corrected future protocol needs content fingerprints of runtime/build/harness inputs and binary identity, plus regression checks for real source additions, deletions and mutations.

The future-driver fix now captures content identities for src, examples, top-level Rust benches, Python/shell scripts, Cargo/build/toolchain files, .cargo configuration and the explicit Lin executable. Generated benchmark report trees are excluded; protocol-specific archived wrappers still require their own frozen-input verification. Git status remains informational. Actual content drift or inability to read the binary marks the run incomplete. Six tests cover generated output, unchanged dirty overlays, source edits/additions/deletions, build/harness inputs, replaced/missing binaries, and finish integration. AST comparison verifies all backend classes and timed helper functions unchanged; only main/finish change. No corrected CI run has been performed yet.
