# Changed-runner MongoDB readiness, no timings

Previous exact-image diagnostic41308932413 on ubuntu24.04 observed kernel7.0.0-1017-azure and MongoDB's SERVER121912 compatibility refusal. No server guard was bypassed.

The [official runner image inventory](https://github.com/actions/runner-images/blob/main/images/ubuntu/Ubuntu2204-Readme.md), inspected2026-11-02, advertises Ubuntu22.04 image20261026.341.1 with kernel6.8.0-1070-azure. This is a hypothesis for a different usable host, not evidence about an actual scheduled job. Ubuntu22.04 is a temporary diagnostic runner choice; the inventory also announces its deprecation. Actual uname, Docker/server versions and cleanup are recorded by this probe.

Keep the previously failed Mongo image digest, bindings, localhost port, tmpfs size and WiredTiger cache setting. Reject the known affected kernel interval6.19.0 through7.0.13 before starting a container; reject unparseable/prerelease kernels. Eight boundary cases and two unknown/prerelease negatives run before Docker. MongoDB retains all its own startup guards. A ready result requires ping, exact insert/find/delete/count, and verified removal of the uniquely labelled owned container. Logs, inspect and status are retained on failure. This checks compatibility/readiness, not durability or comparative performance. Benchmark timing count is zero.
