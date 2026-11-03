# Diagnostic build-log ANSI parsing failure

Run https://github.com/themoretheless/lin/actions/runs/41634863298 at 61dd7cb73ce8fe9ed4e560ccdf5e68eac88a0944 stopped before copying the ordinary ELF and before any measurement. Cargo compiled lin and lin-peer-worker successfully; CARGO_TERM_COLOR=always inserted ANSI escapes between Compiling and package name, so the plain-text regex rejected the log. Zero worker processes and zero timings. All three downloaded members archived and SHA-256 reread verified.

Fix: explicitly pass cargo --color never to clean/build, and normalize ANSI in the independent auditor. Actual compilation remains required. This permits an infrastructure-only diagnostic retry; no numerical result is discarded or retried. Runtime source and the diagnostic instrumentation patch are unchanged.
