# MessagePack positive-zero experiment: CPU gate not accepted

Selection fused into Cell serialization with an exact whole-vector zero count and50% density threshold. Expanded12-case set includes prefix-only zeros,late zeros and mixed sparse/dense vectors. This version fails CPU targets and several controls. Absolute baseline times also differ markedly across separately compiled harnesses;do not compare different revisions as one causal pair.

12 fresh processes,3 roles per process,432 encode blocks,1728000 encodes. Each block averages4000 encodes into a reused64KiB buffer;fixtures and legacy decode checks are outside timing. Two independent six-process series rotate case and role order. Baseline/control call the same canonical serializer in the same executable;candidate calls the view serializer. These are hot serialization-only measurements,not I/O,commit latency,RSS or whole-engine qualification.

All logical Pack/Row data,public Row alias,public serde and quota charging remain unchanged. Only positive f32 zero in selected mutation vectors is represented by existing valid MessagePack integer0;legacy typed decoding restores exact +0 bits. Negative zero,NaN/infinity payloads and non-vector Float cells retain their old representation. Canonical pre-compaction frame length still enforces the16MiB rejection boundary. Original decoder and checksum/frame magic are unchanged.

Actual UPDATE/DELETE fixture:739 positive zeros,canonical payload4112 bytes/frame4125 bytes;compact payload1156 bytes/frame1169 bytes. Existing decoder restores complete canonical record bytes;physical Full-sync append/export decode passes. Smaller bytes do not imply lower sync latency.

CPU acceptance rules: each default UPDATE/DELETE target needs6/6 positive pairs and median gain above median absolute A/A in both series;all controls require gain >= -max(2%,A/A). This gate remains failed. No production enablement or full-engine acceptance is claimed.

| Case | Series | Target | Gain % | A/A % | Positive | Pass |
|---|---|---|---:|---:|---:|---|
| batch | series1 | False | -30.937 | 0.620 | 0/6 | False |
| batch | series2 | False | -30.490 | 0.335 | 0/6 | False |
| dense_update | series1 | False | -6.162 | 1.795 | 1/6 | False |
| dense_update | series2 | False | -6.622 | 0.356 | 0/6 | False |
| doc_delete | series1 | True | -28.046 | 2.093 | 0/6 | False |
| doc_delete | series2 | True | -32.039 | 1.260 | 0/6 | False |
| doc_update | series1 | True | -30.836 | 1.042 | 0/6 | False |
| doc_update | series2 | True | -32.897 | 0.879 | 0/6 | False |
| empty_update | series1 | False | 8.017 | 0.554 | 6/6 | True |
| empty_update | series2 | False | 8.220 | 2.452 | 6/6 | True |
| late_zeros_update | series1 | False | -23.169 | 1.099 | 0/6 | False |
| late_zeros_update | series2 | False | -25.313 | 0.954 | 0/6 | False |
| mixed_vectors | series1 | False | -17.602 | 1.548 | 0/6 | False |
| mixed_vectors | series2 | False | -20.231 | 0.736 | 0/6 | False |
| negative_zero_update | series1 | False | -5.290 | 1.382 | 1/6 | False |
| negative_zero_update | series2 | False | -8.321 | 0.862 | 0/6 | False |
| no_vector_update | series1 | False | 10.966 | 1.416 | 6/6 | True |
| no_vector_update | series2 | False | 7.782 | 0.938 | 5/6 | True |
| prefix_only_zeros | series1 | False | -4.701 | 1.561 | 0/6 | False |
| prefix_only_zeros | series2 | False | -5.390 | 0.964 | 0/6 | False |
| unrelated_insert | series1 | False | -1.144 | 0.838 | 2/6 | True |
| unrelated_insert | series2 | False | 1.053 | 1.696 | 4/6 | True |
| zero_update | series1 | False | -22.442 | 1.955 | 0/6 | False |
| zero_update | series2 | False | -26.747 | 0.668 | 0/6 | False |

Raw blocks,process IDs/loads,compiler/source/binary hashes,helper/oracle code and all failed controls are retained. verify-cpu.py independently recomputes the gate and checks immutable historical core hashes.

The initial density fixture expected every nonzero density to compact,which contradicts the new50% selector. Its failed test/log/status/source are retained as initial-density-fixture files. The final selected fixture keeps every IEEE edge value and adds zeros to select compaction;the nonselected fixture explicitly checks canonical bytes for low zero density.

