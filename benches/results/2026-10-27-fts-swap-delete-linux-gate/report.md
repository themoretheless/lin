# Predeclared full-engine FTS swap-delete gate

Baseline production Rust20591dd4; candidate339290fe FTS/Store snapshots only. No unaccepted undo ownership or bitmap empty-check changes. Local290tests and mutant sensitivity passed. No latency result yet.

Same20native mutation/transaction cases and SQLite measurements, two six-round rotated A/A/B series,8fresh-fixture samples/case. Target is now delete1k/10k/100k because this is an independent deletion mechanism: each target must win6/6 and exceed A/A in both series. Update/transaction/SQLite controls retain the same regression bounds. Earlier update candidates remain failed under their original gates; they are not re-evaluated under this target selection. All peer timings and losses remain visible. Success is a component milestone, not achievement of all-eight-peer or durable performance goals.

Extra retained old token storage is capped at64KiB requested Vec/String capacity, excluding allocator overhead/RSS. Large rows use the sequential path. Before rollout also measure large/disjoint/Unicode/cold/resource cases and the wider read/native/Full-durable contracts.
