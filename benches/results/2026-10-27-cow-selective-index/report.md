# Selective scalar-index maintenance prototype

Effective UPDATE patch skips only non-partial scalar indexes with no patched key/bitmap dependency. Partial predicates always run, including time-dependent predicates. Public Store insertion paths remain unchanged. This is an archived prototype, not production source.

409 default library/integration tests passed, 2 existing ignored. A full-rebuild membership oracle checks unique/composite/partial forward and reverse maps, unique error rollback, and explicit rollback. It fails both intentional mutants: skipping a changed non-partial index, and ignoring partial membership changes. Original COW isolation/wire-format/FTS/hash oracles also pass. All logs retained.

Incremental performance versus the previous COW-inplace source is in ../2026-10-27-cow-selective-index-warm/. Warm CPU UPDATE at 1k/10k/100k: -2.029/-5.339/-0.696%, A/A 2.706/2.956/1.270%, positive pairs 2/6,1/6,1/6. Rollback -2.340/-0.791/+0.354%, A/A 8.438/3.101/1.814%. This diagnostic does not qualify fresh-fixture or peer performance. The 10k slowdown exceeds its A/A; the mechanism is not advanced or shipped. No new Linux run is launched for it.

Fixture-applicability correction from the integrated fresh profile: setup creates only non-unique docs[wing,ts], and UPDATE patches wing. There were no untouched scalar indexes to skip and no scalar unique lookups to bypass in this performance fixture. The semantic unique/partial oracles remain valid, but warm results are not evidence about schemas that actually exercise the proposed skip. Neither mechanism is advanced for the measured target.
