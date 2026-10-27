# Local preparation evidence only

Both uninstrumented binaries built successfully. All 61 cases pass with each binary (122 one-observation correctness smoke processes), including full-value durable reopen and COUNT preflight. This macOS smoke does not execute the Linux reserve implementation and is not a latency measurement. Full raw smoke files are in local-smoke.tar.gz; local-smoke-status.json lists all invocations.

The synthetic 61-case analysis test accepted the complete fixture, rejected summary/RSS/missing-row/NaN corruption, and rejected a losing target pair and a regressing read control at the frozen gate. Synthetic fixtures do not represent engine timings.

Candidate persistence source and oracle are copied byte-for-byte from the transaction-corrected functional revision. Its full local suite previously passed 417 tests with 2 ignored; CI repeats functional validation on real Linux before this timing matrix.
