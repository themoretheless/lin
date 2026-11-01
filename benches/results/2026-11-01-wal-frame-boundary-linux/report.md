# WAL frame boundary diagnostic

216 processes / 1728 observations completed; all stored file bytes verified. Local raw recomputation matches CI. Diagnostic only; no engine optimization accepted.

|Offset|4096 bytes, us|4125 bytes, us|4125 / 4096|
|---|---:|---:|---:|
|0|456.895|474.574|1.0387|
|2048|474.757|479.611|1.0102|
|4095|476.546|474.182|0.9950|

Ratios above are ratios of process-median aggregates, not paired confidence estimates. No uniform boundary penalty is established: 4125 versus 4096 is approximately +3.9%, +1.0%, and -0.5% at the three offsets. Control A/A variation spans about 1.2-8.7% across cases. The fresh-file prefix protocol does not reproduce full database append history. Do not change WAL encoding or add padding based on this result. Keep previous codec and RWF_DSYNC rejections.
