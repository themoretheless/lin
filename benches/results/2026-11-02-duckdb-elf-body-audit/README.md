# Exact DuckDB function-body audit

Diagnostic input: exact Linux ELF artifacts from successful crossed control run 41363558777. SHA256 identities are fixed in `audit.py` and independently checked before parsing. The script reads ELF64 executable load segments, takes nonzero `.text` DuckDB function symbol ranges from LLVM objdump 21.0.0, excludes every duplicated demangled name, then hashes the actual file bytes for each common function.

43,290 common unambiguous functions: all sizes match; 5,914 have identical body bytes and 37,376 differ. Each binary has 4,532 duplicated names excluded from this comparison. The earlier symbol-size diagnostic overwrote duplicate names in a dictionary; its size/address counts must not be treated as a one-to-one mapping of every function. This audit explicitly avoids that ambiguity.

Differences in file bytes include address-dependent instruction operands. This audit does not normalize relocations, compare algorithms, prove changed instruction behavior, or establish a cause for the GROUP BY slowdown. The next useful intervention must isolate layout/initialization behavior and retain the existing workload and correctness oracles.

Reproduce with `python3 audit.py /path/to/exact-executable-artifact-directory`. Full per-function addresses, sizes, hashes and byte equality flags are retained in `function-bodies.json.gz`; the uncompressed payload hash is recorded in `summary.json`. This is static evidence, not a new performance measurement. No runtime candidate was adopted.
