# Supplemental correctness and static diagnostic

This supplement does not change the frozen FTS CI experiment or its acceptance criteria.

`run.py` temporarily appends the exact `supplement.rs` test to `src/exec.rs` to access the transaction's private database. It restores the original bytes in `finally`. The test modifies 300 distinct row positions, explicitly asserts borrowed canonical alpha postings after removal 257 (43 remaining positions), and checks rollback row bytes, generation, and postings against an independent rebuild. It covers explicit/Drop rollback and Rows/Mixed journals. Local release results are functional evidence only.

`duckdb-symbol-layout.json` is a static comparison of the exact crossed-control ELF artifacts from run 41363558777 using Apple LLVM objdump 21.0.0 `--syms --demangle`. There are 47,822 common DuckDB function symbol names; every common size matches and every address differs. This does not establish equal machine code or a causal explanation for the observed regression. Raw symbol listings remain beside downloaded executables in `/tmp/lin-crossed-control-executables-41363558777/`. No executable was run on macOS.

The initial supplemental fixture omitted `body`/`uri`, so lookup of its CAS hash failed before any fold/rollback assertion. This failed attempt is retained as `attempt1-fixture-failure.log` and `attempt1-status.json`; it is not runtime qualification. The corrected fixture supplies immutable body and unique URI. Empty posting results are compared as sets, normalizing absent/empty representations.

The dominant common DuckDB text address shifts are +1328 bytes (42,642 symbols) and +1344 bytes (5,166 symbols). This establishes a layout difference; no alignment intervention or timing qualification has been performed.
