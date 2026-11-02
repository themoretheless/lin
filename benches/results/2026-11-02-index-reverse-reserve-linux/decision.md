# Reject reverse-map reservation candidate

Two independent Linux series completed and raw audit verified all 108 processes, 864 observations, 3996000 exact six-field row readbacks, frozen source blobs and both immutable ELFs. Worker tests and local complete default regression (463 passed / 2 ignored) passed.

Native INSERT100k gains were only +0.350% / +0.649%, with 3/6 / 5/6 positive pairs and AA variation 0.574% / 0.600%. These fail a reliable six-of-six improvement above variation. INSERT1k regressed -0.697% / -1.048%, both 2/6 positive, versus AA 0.684% / 0.856%. INSERT10k was mixed +0.312% / -0.133%. No gain against DuckDB is established; no external engine ran in this experiment.

Reject candidate f2ca04879 and restore runtime src/index.rs byte-for-byte to baseline b88ebea2e. Its four-line patch and original commit remain recoverable. Preserve every loss and control; no unchanged retry, weakened threshold, source mixing or adoption. This result does not prove the mechanism of the regression.

Next qualification should assess the already materially changed combination of COW/Arc keys and extent WAL against the historical complete BTreeMap baseline with all 67 cases and immutable peer controls. This would be a new combined-runtime qualification, not a retry of this reservation change. Its explicit storage format/API, correctness/recovery and remaining rollback losses must remain visible; the SQLite component 77/96 wins is insufficient for full-objective completion.
