# UPDATE patch clone elision: infrastructure attempt, no timings

Run 41249170021, frozen f38e3178c, completed with failure at Build ordinary paired binaries. All 455 release tests / two ignored and both separately compiled hash-guard runtime negatives passed. Independent functional audit verified frozen assets, exact mutant source/logs, source restoration and absence of all smoke/matrix outputs. There are zero measured benchmark processes or observations.

The actual checkout log records --depth=1. build.py needs git show 21dcd7d81:src/exec.rs to compile the baseline; that source history was absent. The same baseline source exists in the full repository. No paired binary was built. The failed step, full job log and complete raw artifact remain preserved with SHA256 and verified archive members.

A single infrastructure retry is justified after fetching baseline history. Runtime source, cases, samples, A/A protocol and acceptance thresholds must stay unchanged. This attempt supports no timing result, component acceptance or peer win. The separately reproduced schema rollback defects remain a whole-engine correctness requirement.
