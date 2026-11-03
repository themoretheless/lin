# Typed numeric columns: local discovery, gate failed

Candidate `29f6a8ed4` versus correctness-aligned `a28aa758b`, with original production `0dab1a9a4` as an additional declared role. Typed numeric tags are maintained on rebuild/register/update/delete; large integer comparisons and nonnumeric cells retain generic comparison semantics. Source-order additions, key identity and quota enforcement remain active. Full workspace413passed/3ignored, including tag maintenance, generic fallback and integer precision above2^53.

All144processes/1008case-observations/21888wall samples retained and independently recomputed.10/12 target windows pass;3 controls fail (count_by1k both series, scalar SUM100k series1). Native control is docs insertion, not an orders-write qualification. No broad local adoption or RSS/first-query claim.

Relative to the correctness baseline, target median gains range34.69–81.07%, but two target windows have only5/6positive pairs. Relative to original production, unfiltered10k improves52.69/53.22% and100k38.20/42.46%;1k unfiltered and all filtered production-relative windows remain slower. These are matched roles in this run, not pooled cross-run results. Failed controls are not waived and no unchanged retry is performed.

A subsequent different source changes the actual scalar SUM and grouped COUNT paths, adds a mutation-invalidated numeric summary and removes repeated whole-column eligibility checks. Its first numerical qualification is separately frozen on Linux; it inherits no acceptance from these data.
