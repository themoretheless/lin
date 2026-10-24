# Streamed embedding text preparation: global substitution not qualified

Source `b58d6eaee76cc79f9f2f17f3cc7fb0b0a1b88ea7`; production unchanged. This tests a different allocation approach from the rejected inline-text enum: reuse one String per batch, immediately embed each row and retain only the resulting Arc vectors. No worker thread, cache, model or precision change.

Baseline mechanically copies current row_embed_text and reproduces retained Cow texts + reference vector + public HashingEmbedder.embed_batch. The diagnostic uses public Row/Cell; local row_text shim is the current get-and-Cell::text operation. Candidate preserves single-field borrowing and join separator rules, reuses scratch for multi-field joins, and calls public HashingEmbedder.embed per row. Thus the comparison includes differences between repeated single embedding and batch scratch access; it does not isolate string allocation as a causal mechanism. Output allocation/destruction and all Row field lookups/preparation are timed. Input fixture construction, model initialization and exact checks are outside.

Six independent release processes, twelve alternating rounds each, identical default 768-dimension hashing model. Cases use default-shaped 1k/10k title+body, single-field 1k guard, and Unicode multi-field 1k. Joined strings and embedding bits match for all 125 combinations of absent/empty/short/Unicode/long fields; vector equivalence checked at dimensions 8/9/128/768/1536 and before timing each fixture. This verifies finite current-model equivalence, not arbitrary third-party backend behavior. No runner builds/tests overlap measurements; host work uncontrolled.

| Case | Batch baseline ms | Stream ms | Paired gain | Faster processes |
|---|---:|---:|---:|---:|
| default_10k | 2.736734 | 2.608516 | +4.684% | 5/6 |
| default_1k | 0.229152 | 0.210931 | +7.927% | 6/6 |
| single_field_1k | 0.164432 | 0.167888 | -2.336% | 0/6 |
| unicode_1k | 2.253935 | 2.210336 | +0.999% | 6/6 |

## Decision

Do not adopt a global streamed substitution: the single-field guard regresses 2.34% in all six processes. Default 1k component gain 7.93% (~18us) is positive but small in absolute whole-operation terms and does not close the durable gap. Default 10k +4.68% and Unicode +1.00% do not establish any native/durable peer gain. No independent repeat, RSS, full engine integration or end-to-end performance acceptance is claimed after the failed global guard.

Any further workload-specific generator capability would need explicit dispatch/fallback design, source-owned borrowed/single-field guards, mixed shapes, third-party batch-override compatibility, workspace and A/A-calibrated native/durable evidence. Existing Embedder::embed_batch overrides cannot simply be replaced by repeated embed calls. This standalone Hashing-only experiment does not introduce that API or dispatch policy. All diagnostic/reference/candidate sources, hashes and raw timings retained, temporary example removed.

Production/default embedding and accepted read/native evidence remain unchanged. Full named-peer objective remains incomplete: durable 1k SQLite gap, MSSQL/Kusto live qualification and outstanding contracts.
