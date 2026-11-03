# Actual orders SoA grouped SUM: correctness repair

Baseline `0dab1a9a4` fails two valid public API regressions: grouped SUM ignores max_aggregate_bytes, and parallel partial sums produce0.0 where the generic source-order path produces1.0. The first fixture attempt failed FK/type validation and is retained separately; it is not counted as runtime evidence. Corrected fixtures create FK parents and exercise orders_soa_ready before the named failing assertions.

Repair `a28aa758b` sums the cached numeric values in source row order, retains Cell identity, charges the generic distinct-key quota and returns owned result keys. Full workspace410passed/3ignored. Additional generic-path tests remain useful correctness regressions even though the earlier generic-only optimization did not exercise the timed orders path.

Independent local108-process/648-case/15552-sample audit: only2/12 target windows pass, with3 control failures. Unfiltered10k improves18.149/19.525%, but filtered and other sizes incur substantial costs. This is a necessary correctness repair with unsuccessful broad performance discovery, not an accepted acceleration. All data retained. The next candidate uses typed column facts and cached text group keys; it is a different source, not a rerun of this one.
