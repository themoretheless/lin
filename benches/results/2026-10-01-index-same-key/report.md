# Posting-offset reverse index experiment — rejected

Three alternating process pairs, 12 fresh fixtures per case, one operation per fixture. Exact absolute timestamp fixtures and readback checks unchanged. Pinned optimized binaries; tests/builds did not overlap measurement. Host load uncontrolled. Numbers in microseconds.

Candidate stores (key, posting offset) in the reverse hash map and updates the swapped tail offset on removal. This removes linear posting search but enlarges reverse-map entries.

| Case | Baseline process medians | Candidate process medians |
|---|---|---|
| compare/update_1row_1k/lin | 4.583, 12.417, 10.062 | 4.708, 10.396, 8.791 |
| compare/delete_1row_1k/lin | 9.979, 10.271, 14.916 | 10.792, 16.104, 14.584 |
| compare/update_1row_10k/lin | 8.416, 13.188, 12.000 | 9.396, 13.708, 14.854 |
| compare/delete_1row_10k/lin | 16.541, 27.917, 20.375 | 14.334, 24.749, 21.562 |
| compare/update_1row_100k/lin | 13.416, 20.625, 20.729 | 23.375, 22.938, 19.584 |
| compare/delete_1row_100k/lin | 35.396, 53.771, 46.541 | 35.604, 33.646, 26.709 |

100k delete improved 2/3 pairs, but 10k update regressed 3/3 and 100k update regressed 2/3. Results do not establish an overall write improvement. Candidate rejected and original production representation restored. Unchanged SQLite controls and raw observations are retained. No all-peer superiority established.

Validation adds a 3000-operation model for reverse offsets and checks stored positions against forward postings; rebuilding may produce different posting order. Candidate source retained as candidate-index.rs.
