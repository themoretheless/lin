# Bulk map-template hypothesis

Candidate a94bab306 is based on preceding COW+CAS a7d4b02c3 and excludes the pending SoA text-reuse candidate. It changes only the local batch-shape template from Row to BTreeMap<String,Cell>, fills the cloned map, then wraps it as Row. Fallback record construction, duplicate-field ordering, timestamp evaluation, parallel worker behavior, WAL and Row representation are unchanged.

Ten focused release record-construction tests pass, including reference equivalence, source retention, wide/empty fallback, duplicate keys across worker boundaries, worker-spawn failure, generated IDs/order and timestamps. Full release tests and paired performance qualification remain pending.

The hypothesis is that explicit owned-map cloning/filling may avoid COW enum dispatch before the map becomes a Row. No assembly or timing evidence currently proves that this dispatch is a material cost. This candidate addresses the bulk construction path only; it does not establish a fix for filter_eq or durable-write regressions. No compiler inlining annotations or unsafe layout tricks are added. No unchanged rejected timing run is repeated.
