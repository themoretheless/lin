# Incremental selective-index warm CPU control

Baseline is the previous COW-inplace candidate with owner-PID lock fix, not main205. Candidate adds only selective non-partial scalar-index maintenance. See variant source hashes in provenance.json. Six rotated A/A/B rounds, 18 processes, 16 samples of 512 alternating prepared UPDATEs and one explicit rollback per size (1k/10k/100k), exact readback after every sample. This warm fixed-timestamp diagnostic is not fresh-fixture acceptance.

UPDATE gains: -2.029/-5.339/-0.696%, versus A/A 2.706/2.956/1.270%; positive pairs 2/6,1/6,1/6. Rollback -2.340/-0.791/+0.354%. Reject this mechanism at this stage; keep all samples and controls. Do not rerun unchanged seeking positive results. Production sources restored.
