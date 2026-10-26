# Partial-index predicate exit removes old membership

Before: LiveIndex::insert_at returned immediately when an updated row failed the partial predicate. If that slot was previously indexed, the forward/reverse entry and unique-key reservation remained live. Reproducer: unique title index restricted to wing=rag; row a leaves rag, then row b should be allowed to claim the same title. Two membership regressions fail before the fix.

After: remove_at(idx) before returning for a nonmatching replacement. The append-only insert_at_new path is unchanged. Existing remove_at removes both reverse membership and the forward posting, dropping an empty key. The normal non-partial/matching key path remains unchanged.

Four targeted regressions pass: unchanged-key exit and unique-key transfer; exit combined with key change; failed transfer rollback (rows/generation/index memberships); WAL export/apply plus checkpoint/reopen, including rejection of a later conflicting entrant. Wider validation passed278 tests with2 existing ignored; exact command, frozen source hashes, before/after logs retained. These are correctness results, not a measured latency improvement or physical power-loss proof.

The independent borrowed-row performance candidate remains a saved experiment. Linux A/A/B run40390146614 tests its frozen snapshot, which does not include this separate partial-index fix. Do not carry that future result to a combined binary without requalification.
