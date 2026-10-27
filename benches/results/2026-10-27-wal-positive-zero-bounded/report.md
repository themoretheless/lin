# MessagePack positive-zero experiment: CPU gate not accepted

Bounded probe: first row,first4 cells,first8 values of the first vector;at most2 nested first Batch packs. No record content is removed;missed zeros use canonical encoding. Default-row encoding improves about20-22%,dense controls remain within limits,but empty/no-vector encoding regresses about3% (roughly5 ns).

12 fresh processes,3 roles per process,324 encode blocks,1296000 encodes. Each block averages4000 encodes into a reused64KiB buffer;fixtures and legacy decode checks are outside timing. Two independent six-process series rotate case and role order. Baseline/control call the same canonical serializer in the same executable;candidate calls the view serializer. These are hot serialization-only measurements,not I/O,commit latency,RSS or whole-engine qualification.

All logical Pack/Row data,public Row alias,public serde and quota charging remain unchanged. Only positive f32 zero in selected mutation vectors is represented by existing valid MessagePack integer0;legacy typed decoding restores exact +0 bits. Negative zero,NaN/infinity payloads and non-vector Float cells retain their old representation. Canonical pre-compaction frame length still enforces the16MiB rejection boundary. Original decoder and checksum/frame magic are unchanged.

Actual UPDATE/DELETE fixture:739 positive zeros,canonical payload4112 bytes/frame4125 bytes;compact payload1156 bytes/frame1169 bytes. Existing decoder restores complete canonical record bytes;physical Full-sync append/export decode passes. Smaller bytes do not imply lower sync latency.

CPU acceptance rules: each default UPDATE/DELETE target needs6/6 positive pairs and median gain above median absolute A/A in both series;all controls require gain >= -max(2%,A/A). This gate remains failed. No production enablement or full-engine acceptance is claimed.

| Case | Series | Target | Gain % | A/A % | Positive | Pass |
|---|---|---|---:|---:|---:|---|
| batch | series1 | False | 22.061 | 1.264 | 6/6 | True |
| batch | series2 | False | 21.557 | 0.650 | 6/6 | True |
| dense_update | series1 | False | -0.764 | 1.031 | 0/6 | True |
| dense_update | series2 | False | 0.256 | 0.890 | 4/6 | True |
| doc_delete | series1 | True | 21.079 | 0.214 | 6/6 | True |
| doc_delete | series2 | True | 21.680 | 0.465 | 6/6 | True |
| doc_update | series1 | True | 20.466 | 0.544 | 6/6 | True |
| doc_update | series2 | True | 21.143 | 0.819 | 6/6 | True |
| empty_update | series1 | False | -3.788 | 2.608 | 0/6 | False |
| empty_update | series2 | False | -2.801 | 0.704 | 0/6 | False |
| negative_zero_update | series1 | False | 0.593 | 0.537 | 4/6 | True |
| negative_zero_update | series2 | False | -0.921 | 0.347 | 0/6 | True |
| no_vector_update | series1 | False | -3.442 | 1.890 | 0/6 | False |
| no_vector_update | series2 | False | -2.914 | 0.926 | 1/6 | False |
| unrelated_insert | series1 | False | 0.047 | 0.732 | 3/6 | True |
| unrelated_insert | series2 | False | -0.132 | 0.567 | 2/6 | True |
| zero_update | series1 | False | 24.704 | 1.584 | 6/6 | True |
| zero_update | series2 | False | 24.679 | 0.389 | 6/6 | True |

Raw blocks,process IDs/loads,compiler/source/binary hashes,helper/oracle code and all failed controls are retained. verify-cpu.py independently recomputes the gate and checks immutable historical core hashes.

Full local default release suite with this candidate:435 passed,2 ignored. A separate end-to-end experiment adds real Full/Normal scalar updates to all prior61 cases to test whether smaller I/O outweighs the known CPU tradeoff;it does not relabel this failed CPU gate.

