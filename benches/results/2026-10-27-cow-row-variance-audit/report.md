# COW failed-pair resource audit

Read-only analysis of run40421852372 raw status/summary. audit.py verifies input hashes and saves every failed target pair, no exclusions or gate changes.

Five UPDATE pairs nonpositive:1k rounds7and11,10k round7,100k rounds3and7. Round7 baseline/control/candidate1minute load is approximately1.002/1.006/1.004;round11 all1.0;round3 approximately1.424/1.303/1.216. Across all36processes recorded pre-run1minute load ranges1.0..3.914. Most failed target pairs therefore do not coincide with elevated recorded load.

Load is neither CPU utilization nor CPU steal,frequency,affinity,cache-miss or allocator telemetry. No claim of an overloaded runner or causal proof follows from high A/A. COW remains unqualified; all five losses and every control stay in acceptance. No unchanged retry is authorized by this analysis alone.

Next diagnostic should instrument phases on actual Linux fresh-input compare fixture,including setup/readback boundaries and Handle drop,with raw per-operation timings separated from acceptance wall values. Existing macOS fixed-timestamp phase profile and repeated-row CPU result cannot establish which Linux fresh-input phase dominates. Public API,read/native/durable/RSS qualifications remain open independently of variance.
