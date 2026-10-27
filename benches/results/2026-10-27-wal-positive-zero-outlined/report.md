# Outlined compact WAL serializer: CPU gate rejected

New hypothesis after the failed full-engine gate: separate compact encoding from the canonical fallback with an explicit non-inlined function. No payload, zero eligibility, canonical frame budget, decoder, sync policy or fixture was changed. Baseline core includes the schema-WAL rejection correction; exact hashes are in test-status.json.

Prior local full-engine executables show an out-of-line encode symbol already existed. Its distance to the next text symbol was1380bytes; baseline append_record_v1 was544bytes. These symbol distances are local Mach-O observations, not exact Linux function sizes, cache profiles or a proven cause of COUNT regressions. Whole symbol lists, binary hashes and selected entries are retained.

Five focused WAL tests pass.12fresh release processes,9cases ×3roles,324blocks ×4000encodes =1296000encodes. Independent verify-cpu.py recomputes all pairs/medians/A-A and source provenance; CPUgate=false. No full-engine or performance adoption is claimed.

| Case | Series | Gain % | Absolute A/A % | Positive | Pass |
|---|---|---:|---:|---:|---|
| batch | series1 | 21.685 | 0.307 | 6/6 | True |
| batch | series2 | 21.378 | 0.198 | 6/6 | True |
| dense_update | series1 | -0.159 | 0.175 | 1/6 | True |
| dense_update | series2 | -0.606 | 0.162 | 0/6 | True |
| doc_delete | series1 | 21.463 | 0.175 | 6/6 | True |
| doc_delete | series2 | 21.889 | 0.552 | 6/6 | True |
| doc_update | series1 | 21.769 | 0.218 | 6/6 | True |
| doc_update | series2 | 21.513 | 0.367 | 6/6 | True |
| empty_update | series1 | -3.609 | 1.177 | 1/6 | False |
| empty_update | series2 | -1.747 | 0.565 | 0/6 | True |
| negative_zero_update | series1 | -0.618 | 0.161 | 0/6 | True |
| negative_zero_update | series2 | -0.647 | 0.067 | 0/6 | True |
| no_vector_update | series1 | -4.133 | 0.753 | 0/6 | False |
| no_vector_update | series2 | -4.027 | 0.189 | 0/6 | False |
| unrelated_insert | series1 | -0.158 | 0.217 | 3/6 | True |
| unrelated_insert | series2 | 0.508 | 0.526 | 4/6 | True |
| zero_update | series1 | 24.960 | 0.450 | 6/6 | True |
| zero_update | series2 | 25.321 | 0.380 | 6/6 | True |

The target serializer gains remain positive, but no-vector regressions remain about4% in both series and the empty-vector control fails series1. This does not resolve the known CPU tradeoff and does not establish whether layout caused full-engine COUNT regressions. No additional full-engine run is launched for this revision. Different compiled harness revisions are not a causal before/after pair; compare each revision only against its own baseline/control roles. Production source is restored.
