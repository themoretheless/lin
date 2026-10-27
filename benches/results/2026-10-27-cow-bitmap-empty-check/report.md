# Naive bitmap emptiness short-circuit: not accepted

COW on main047 with take-all and Eq/Hash; only PostingList::is_empty delegates Bitmap to BitSet::is_empty, and two remove paths use is_empty instead of count()==0. Full default tests pass (test-summary.json). Independent BTreeSet oracle covers dense/sparse membership, promotion/growth, absent removal and last-bit deletion.

Incremental warm UPDATE1k/10k/100k: -1.278/+1.976/+11.977%, A/A2.871/1.962/0.497%, positive2/6,4/6,6/6. Rollback+2.577/+1.111/+7.477%. Despite the100k benefit, direct CPU controls show27–41% slower last-word/fully empty bitmap checks versus the original vectorized count. Do not advance this naive all-word .all implementation. Exact raw controls in ../2026-10-27-bitmap-empty-cpu/; all warm observations in ../2026-10-27-cow-bitmap-empty-warm/. No fresh-fixture or peer acceptance.
