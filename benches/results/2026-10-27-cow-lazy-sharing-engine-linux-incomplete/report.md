# Incomplete lazy COW full-engine run

Run [40516846890](https://github.com/themoretheless/lin/actions/runs/40516846890), source 3d4df6624ff444e53309b876b07f052b321ca021, conclusion cancelled. Functional and build steps succeeded; timing step was cancelled at roughly 61 minutes from job start with a configured 60-minute job timeout. This is consistent with timeout, not a performance gate failure.

Downloaded artifact contains 1207 status-final.json files out of 1332 required processes. These files have not all been independently audited for completion. Summarization, independent audit and final gate steps were skipped. No full-engine qualification, accepted speedup or peer win is claimed.

partial-artifact.tar.gz preserves the downloaded directory including inputs and partial raw measurements. raw-manifest.json hashes every unpacked file. Full data remains unpacked locally. A future run needs adequate job time and a baseline including the transaction WAL correction; unchanged old results cannot qualify the new binary.
