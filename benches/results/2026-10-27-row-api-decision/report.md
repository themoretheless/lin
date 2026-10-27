# Reviewable COW Row API decision

Production README.md and src/lib.rs declare Row in the stable public surface. Current Row is exactly BTreeMap<String,Cell>; preserving that identity conflicts with sharing its container via an Arc newtype. No production contract or source is changed by this audit. The user preference question concerns a future API version, not permission to claim performance or deploy this prototype.

## Concrete existing candidate and evidence

Frozen COW/in-place/bounded-bitmap prototype134417eb21f9f9777c26f102c08b163d431f7fe3 passed its720-process native mutation gate relative to production047: UPDATE1k+68.597/+70.044%,10k+45.817/+46.016%,100k+48.150/+49.530%, each series6/6 positive and all controls pass. Exact report: ../2026-10-27-cow-bitmap-prefix-isolated-linux/report.md. This is a compound COW/in-place/bitmap candidate, not proof that Arc alone explains the full gain. It wins only39/120 same-case SQLite comparisons; DELETE remains0/36. Read, insertion, Full-durable, memory/platform/all-features and broader peers remain unqualified. That evidence cannot be attached to a future modified adapter binary without new qualification.

## Actual downstream-style compile checks

Apply the exact four saved COW candidate source files temporarily, then compile the same external-style example on production baseline and prototype. All eight expected outcomes verified; compiler logs and source hashes retained. Sources and temporary example restored. Compile checks do not execute runtime behavior.

| Client usage | Baseline exact map | Frozen COW Row |
|---|---|---|
| Explicit From<Map>, borrowed &Map coercion, entry mutation, owned iteration, Eq/Hash/Send/Sync bounds | compiles | compiles |
| let row: Row = BTreeMap::new() | compiles | fails(E0308): requires explicit conversion |
| row.into_keys() | compiles | fails(E0507): consuming map method cannot move through Deref |
| let map: BTreeMap<String,Cell> = row.into() | compiles | fails(E0277): missing From<Row> for map |

The latter two gaps can be addressed with explicit owning adapters in a future prototype, including unique/shared ownership tests; they have not been implemented here. Exact implicit type assignment remains inherently incompatible with a newtype. Deref does not make a local wrapper the same Rust type. This four-case audit is not exhaustive downstream API compatibility.

## Choices to review

1. Keep exact BTreeMap identity. Continue compatible optimization; preserve the COW qualified result as an unavailable design comparison, not a shipping change.
2. Permit a documented future COW API version. Add/test owning conversions and migration examples, audit all public signatures/derive/generic bounds and held-result isolation, then rerun correctness and full-engine/native/Full-durable/read/memory/peer gates on the final integrated source. Keep old API supported during migration only if that product requirement is chosen; converting to owned maps can reintroduce clone cost and must be measured.

No choice has been inferred from elapsed time or an unanswered question. Until direct user preference arrives, production Row stays the exact alias. The eight-peer objective remains active and incomplete either way.
