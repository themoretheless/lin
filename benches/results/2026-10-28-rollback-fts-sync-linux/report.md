# Rollback FTS synchronization: rejected

Frozenf9e5db28e, run40652603678 completed all288processes/2304observations. Independent raw/source audit passed: only the declared FTS restoration change,454release tests/2ignored,16fresh Linux correctness smokes and the compiled missing-FTS runtime negative. The earlier infrastructure attempt remains separate.

Rollback1k median gains12.269%/19.156%; positive5/6and6/6; A/A2.542%/7.685%. Rollback10k gains14.666%/14.753%; positive6/6both; A/A3.169%/1.363%. The first1k target window fails the frozen6/6criterion. UPDATE1k control series2 also fails:gain-5.151%,A/A3.329%,positive2/6. No threshold relaxation or unchanged retry. Neither the losing target nor the control regression has a uniquely established cause.

All observations, including losing pairs, remain in the verified raw archive. See the full16-window table in rollback-fts-sync/report.md inside the archive/unpacked local data. Gains are relative to corrected baseline21dcd7d81, not peer wins or full-source acceptance. No memory measurement was made.

The candidate2e5146fce is rejected and store.rs is restored byte-for-byte to21dcd7d81. The three validated rollback correctness fixes remain. The full eight-peer goal, full-source failed controls and SQLite FULL WAL gaps remain open. The positive-zero MessagePack Cell probe is separate and has no runtime adoption or timing qualification.
