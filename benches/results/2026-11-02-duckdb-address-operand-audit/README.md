# Address-operand classification of exact DuckDB binaries

Inputs are the SHA256-verified baseline/candidate Linux ELF artifacts from crossed-control run 41363558777 and the hashed per-function records of the preceding ELF-body audit. Only unambiguous common nonzero function ranges are compared; ambiguous names remain excluded.

For each function with different raw bytes, Capstone decodes the complete function and records instruction IDs and lengths. The normalizer masks only encoded immediate operands of CALL/JUMP-group instructions and RIP-relative memory displacements. Arithmetic immediates and every other byte remain visible. A function is classified as address-operands-only only if both ranges decode completely, instruction IDs/lengths match and the masked bytes match. This is a syntactic classification, not semantic equivalence: branch/data target identities and effects have not been proved equal. No cause of the measured latency regression is established.

`test-normalizer.py` extracts the actual normalizer AST from `audit.py`. Three address-only positive pairs pass (including SSE RIP-relative addressing); arithmetic-immediate and opcode changes remain visible; truncated decoding is rejected. Results are in `normalizer-tests.json`. Python distribution capstone 5.0.9 reports internal version 5.0.7; both scripts record the internal version actually used.

Run with Capstone installed in the Python environment:

```
python3 test-normalizer.py
python3 audit.py /path/to/exact-executable-artifact-directory
```

No Lin runtime code, benchmark workload, or acceptance criterion was changed. No performance measurement is represented by this static audit.

The first classification used the decoder-provided displacement width directly. The 85 residual functions (142 instructions) exposed an internal Capstone 5.0.7 metadata defect: some SSE RIP-relative operands report disp_size=2 although the instruction contains disp32. Attempt1 source, counts and instruction details are retained in `attempt1-decoder-width/`. The corrected normalizer requires 64-bit addressing and validates the signed four-byte encoded displacement against every decoded RIP-relative operand before masking it. A dedicated SSE regression pair is included.

Final corrected result: all 43,290 functions decode completely with matching instruction IDs and lengths. 5,914 have identical raw bytes; the other 37,376 match after masking only branch immediates and validated RIP-relative disp32 fields. Zero functions retain other byte differences. This narrows the investigation toward address/layout, data targets and initialization effects; it does not prove target identity or explain the observed slowdown. `detail-remaining.py` confirms zero residual functions in the corrected result.

The locked airbug dependency (75b1321b5d7ff2cee983379f539c07726a7d27e5, `bench/src/convenience.rs` lines79–98) allocates `Rc<OnceCell<RefCell<T>>>` at fixture registration, before initialization. The frozen comparison harness registers Lin fixtures before the selected DuckDB case. Whether actual allocation sizes differ between the exact source pair remains unverified; lazy initialization alone does not rule this path out.
