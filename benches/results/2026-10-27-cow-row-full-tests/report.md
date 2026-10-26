# COW Row full default library/integration tests

cargo test --lib --tests exits0 on macOS ARM64 with default features. 406 passed,2existing ignored across24test summaries; no failed tests. Full log retained. Candidate source is frozen cow-row prototype, no later optimization applied. Includes COW isolation/wire-shape oracle plus default integration suites for mutations,transaction rollback,persistence,streaming/cursors,security,replication,snapshots and query semantics according to actual target logs.

This does not mean all feature combinations,physical GPU,all workspace packages,release performance,external API or historical readers were qualified. Tests excluded by cfg are not executed. Ignored tests stay ignored. Production source restored exactly. Full Linux fresh-input A/A/B qualification40421852372 remains separate and pending; no benchmark win inferred from green tests.
