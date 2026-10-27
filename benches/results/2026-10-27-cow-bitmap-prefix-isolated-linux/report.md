# Integrated COW plus bounded bitmap prefix: native mutation gate passes

Run40443994876, frozen134417eb21f9f9777c26f102c08b163d431f7fe3. Baseline current main047. Candidate COW-inplace + Eq/Hash restoration + bounded first-word scalar-posting emptiness. No FTS empty-destination prototype, index skipping or unique-check reorder.

All720case-isolated processes and5760observations,240triple comparisons and120candidate/SQLite comparisons were independently recomputed. Both six-round series meet the unchanged native mutation gate: UPDATE1k +68.597/+70.044%,10k +45.817/+46.016%,100k +48.150/+49.530%; each target series6/6positive and above A/A. All unchanged controls pass their non-regression bounds. Correctness, hash-undo and bitmap wrong-answer sensitivity checks pass. Original failures of earlier sources/protocols are retained separately and remain failures.

This result qualifies the frozen native mutation candidate relative to main, not the full objective. Candidate beats SQLite39/120times: UPDATE1k11/12,10k1/12,100k7/12; DELETE all sizes0/12;tx_commit1k11/12,10k3/12;rollback1k6/12,10k0/12. Missing named peers and Full durable scenarios are not wins. Comparisons use nearby separate processes.

COW Row is a public newtype rather than an exact BTreeMap alias; Eq/Hash restoration does not remove its source compatibility break. The helper retains a measured ~0.25ns cost on1024-bit sparse/empty kernel fallbacks; first-word dense controls improve strongly. Broader read/native/durable, platform/API and all-features checks remain before production rollout. Future formatted/integrated PR binaries must not be claimed identical to this frozen artifact without verification. FTS empty-destination correctness prototype is separate and still has no performance result.
