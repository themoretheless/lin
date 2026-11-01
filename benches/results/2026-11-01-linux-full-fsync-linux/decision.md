# Linux Full fsync candidate rejected

Original run https://github.com/themoretheless/lin/actions/runs/41341179993 completed. All 720 fresh processes / 5760 wall observations, 458 release tests / 2 ignored, 40 functional smokes, actual syscall check and compiled missing-barrier negative passed. The independent raw/source/command audit passed. Only the frozen component performance gate failed.

0/14 target windows passed; all control windows stayed within their tolerance. Target median gains range from -6.571% to +2.952%; no target had the required six positive pairs above absolute A/A. Do not interpret control tolerance as proof of no regression.

The candidate's direct numeric ratios against SQLite WAL FULL show only 1/72 Lin wins. Median paired Lin/SQLite ratios: UPDATE 1k 1.233848, DELETE 1k 1.242110, COMMIT 1k 1.247003; UPDATE 10k 1.717770, DELETE 10k 1.675320, COMMIT 10k 1.648522. These compare different fresh processes in this original matrix; they are not a new confidence qualification and cannot establish the cause of differences versus older runs.

Keep original fdatasync. No unchanged retry, acceptance waiver, production adoption or main merge. The separate columnar Batch prototype explicitly excludes fsync. All 3841 downloaded files are retained in raw.tar.gz, verified against raw-manifest.json; numerical summaries were recomputed from the downloaded observations and matched. Compiled Linux binary payloads were not uploaded: CI validated their hashes live, while this local archive audit does not rehash payloads it does not contain. The full eight-peer objective remains open.
