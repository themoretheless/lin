# Bounded first-word bitmap emptiness prototype

Current main047 plus COW-inplace/take-all/Eq/Hash. A nonzero first bitmap word proves nonempty; otherwise use the original vectorized count()==0. Standard vectors use Vec::is_empty. Two scalar-index removal sites call the helper. No cached cardinality, layout change, omitted index maintenance or global change to BitSet::is_empty.

Full default library/integration tests pass (test-summary.json), including dense/sparse growth and final-bit BTreeSet oracle. The oracle fails both intentional wrong-answer mutants; logs distinguish assertion failures from compilation failures. Source restored after every operation.

Incremental warm UPDATE1k/10k/100k versus the same integrated COW without this helper: -0.017/+3.172/+12.620%, A/A1.957/2.149/1.426%, positive3/6,6/6,6/6. Rollback -0.720/+3.042/+8.163%, A/A4.226/2.253/1.813%. This is warm-only evidence, not fresh-fixture acceptance. Bounded fallback CPU controls remove the naive27–41% regressions at large bitmaps;1024-bit fallback is2.004ns→2.256ns (~0.25ns slower), while the two Standard cases differ by<0.01ns. All relative percentages and absolute times remain in ../2026-10-27-bitmap-prefix-empty-cpu/. No general no-regression or peer-win claim is made.

Candidate includes the public COW Row newtype source-compatibility break despite Eq/Hash restoration; exact BTreeMap assignments still need explicit conversion. Default-feature tests are not all-features/GPU validation. Further fresh-fixture and broader peer/durable qualification is required before production rollout.
