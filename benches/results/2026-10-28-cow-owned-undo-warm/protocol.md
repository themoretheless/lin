# Owned COW undo vs previous COW

New source705dbd035 vs prior COWa9d50c155, not BTreeMap main. Same uninstrumented warm probe and36processes/3456observations, two six-pair series,1k/10k/100k. Rollback is the target (6/6positive and median gain>A/A each window); prepared UPDATE is an unchanged-path control (gain>=-max(2%,A/A)). This is a new source/mechanism comparison, not a rerun of rejected prior timings. Full canonical row,indexed count,committed generation verified; Handle/Pack/Drop inside timer. No full-engine or peer acceptance. Preserve losses.
