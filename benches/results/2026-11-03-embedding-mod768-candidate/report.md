# Exact default-dimension modulo specialization — candidate

Different from the rejected caller-participation candidate: thread counts/chunking and all encoder paths remain unchanged. Scratch feature hashing keeps the same FxHasher output and exact modulo mapping. At v.len()==768, spelling the divisor as a constant lets code generation avoid variable integer division; every other dimension keeps its original modulo. No model weights, dimension, output bits, API or storage codec change is intended.

Independent dense reference-bit tests now include dimensions 767/768/769 along with existing 8/9/32/384/1024/1536 coverage and all ASCII/control/Unicode whitespace inputs. Default workspace: 414 passed, zero failed, three ignored. Ordinary optimized compare benchmark built successfully. Baseline ARM64 disassembly contains a udiv in Scratch::bump and bump_token; this is hypothesis support, not measured acceleration.

Performance remains unqualified. Next requirement is a prospectively frozen baseline/A-A/candidate durable 1k target with durable 10k and unchanged SQLite FULL WAL controls, retaining all failures. Native DuckDB 10k/100k, full source/read controls and all other peers remain open. Kusto access is still absent; the full objective is not achieved.
