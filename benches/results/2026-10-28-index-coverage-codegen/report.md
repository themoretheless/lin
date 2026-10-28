# Index coverage codegen diagnosis

Source baseline a7d4b02c3 versus rejected allocation-free source884ebeb81, with only index.rs overlaid. Current checkout restored byte-for-byte. Local macOS ARM64 nightly1.101 release-library LLVM IR, not final Linux benchmark code, timing or allocation measurement.

Both optimized modules retain exactly one separate index_covers_pred definition and18direct calls from the same caller-symbol multiset. Coverage function text grows from628to805IRlines; these are text lines,not machine instructions,code bytes or an instruction-cache measurement. Initial analysis incorrectly required identical full caller headers/order; it failed,was preserved in analyze-initial.py/analysis-initial-error.log,and was replaced by explicit caller-symbol comparison.

The evidence does not support treating whole-function outlining as a new established optimization. It does not rule out partial inlining elsewhere,explain the historical Linux point_get regression or prove unchanged final machine code. Do not claim forcing inline(never) fixes point_get. A changed optimization would need a concrete compiled-code mechanism and fresh unchanged controls. The prior coverage candidate remains rejected; this diagnostic does not waive its gate or rerun its timing.

Both complete raw IR modules and compilation logs are archived with per-member SHA256; relevant coverage bodies and call contexts are separately saved. Source hashes,compiler details,commands andtarget layout retained. No runtime source changes or performance qualification. Original eight-peer objective remains open.
