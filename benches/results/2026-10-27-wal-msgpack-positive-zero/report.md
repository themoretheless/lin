# MessagePack positive-zero experiment: CPU gate not accepted

Unconditional mutation-row view. Default-row encoding improves, but dense and negative-zero vectors regress about23-27%;one unrelated-insert control also fails.

12 fresh processes,3 roles per process,324 encode blocks,1296000 encodes. Each block averages4000 encodes into a reused64KiB buffer;fixtures and legacy decode checks are outside timing. Two independent six-process series rotate case and role order. Baseline/control call the same canonical serializer in the same executable;candidate calls the view serializer. These are hot serialization-only measurements,not I/O,commit latency,RSS or whole-engine qualification.

All logical Pack/Row data,public Row alias,public serde and quota charging remain unchanged. Only positive f32 zero in selected mutation vectors is represented by existing valid MessagePack integer0;legacy typed decoding restores exact +0 bits. Negative zero,NaN/infinity payloads and non-vector Float cells retain their old representation. Canonical pre-compaction frame length still enforces the16MiB rejection boundary. Original decoder and checksum/frame magic are unchanged.

Actual UPDATE/DELETE fixture:739 positive zeros,canonical payload4112 bytes/frame4125 bytes;compact payload1156 bytes/frame1169 bytes. Existing decoder restores complete canonical record bytes;physical Full-sync append/export decode passes. Smaller bytes do not imply lower sync latency.

CPU acceptance rules: each default UPDATE/DELETE target needs6/6 positive pairs and median gain above median absolute A/A in both series;all controls require gain >= -max(2%,A/A). This gate remains failed. No production enablement or full-engine acceptance is claimed.

| Case | Series | Target | Gain % | A/A % | Positive | Pass |
|---|---|---|---:|---:|---:|---|
| batch | series1 | False | 19.309 | 1.860 | 6/6 | True |
| batch | series2 | False | 19.222 | 0.927 | 6/6 | True |
| dense_update | series1 | False | -23.337 | 0.797 | 0/6 | False |
| dense_update | series2 | False | -24.638 | 1.655 | 0/6 | False |
| doc_delete | series1 | True | 19.854 | 2.990 | 6/6 | True |
| doc_delete | series2 | True | 20.044 | 1.242 | 6/6 | True |
| doc_update | series1 | True | 16.295 | 2.491 | 6/6 | True |
| doc_update | series2 | True | 16.711 | 0.884 | 6/6 | True |
| empty_update | series1 | False | 1.728 | 1.558 | 5/6 | True |
| empty_update | series2 | False | -0.160 | 2.071 | 3/6 | True |
| negative_zero_update | series1 | False | -26.656 | 1.270 | 0/6 | False |
| negative_zero_update | series2 | False | -24.976 | 1.036 | 0/6 | False |
| no_vector_update | series1 | False | 5.522 | 2.961 | 6/6 | True |
| no_vector_update | series2 | False | 1.211 | 3.440 | 4/6 | True |
| unrelated_insert | series1 | False | -2.531 | 1.143 | 2/6 | False |
| unrelated_insert | series2 | False | -0.574 | 1.660 | 2/6 | True |
| zero_update | series1 | False | 23.275 | 1.819 | 6/6 | True |
| zero_update | series2 | False | 23.471 | 0.675 | 6/6 | True |

Raw blocks,process IDs/loads,compiler/source/binary hashes,helper/oracle code and all failed controls are retained. verify-cpu.py independently recomputes the gate and checks immutable historical core hashes.

