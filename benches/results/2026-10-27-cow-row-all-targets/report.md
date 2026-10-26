# COW Row all-targets compile audit

Frozen candidate source matches cow-row prototype f9def96d. cargo check --all-targets completed exit0 on macOS ARM64 with default features. This includes compilation/type checking of library,binary,examples,benches and test targets in the selected default package. It does not execute those tests or benchmark measurements, does not check all feature combinations, physical GPU or every workspace package.

Initial dependency rebuild included bundled libduckdb-sys and mysql; no alternative peer timings were inferred. Full log,exit status,candidate SHA and source-restoration verification retained. Production src/store.rs,exec.rs,check.rs restored exactly to baseline after operation.

Normal in-repository usage compiles, but external exact-type BTreeMap assignments and foreign/local trait combinations remain a public API break because Row is a newtype in the prototype. From<BTreeMap> exists as an explicit conversion and is not an implicit coercion. Do not label the API backwards compatible from an all-targets check. Performance remains pending Linux40421852372; no production rollout or broad peer completion established.
