### Bench budget (same-run peers and legacy platform limits)

Run: linux/x86_64; legacy: macos/aarch64.
Same OS/architecture does not imply identical hardware.

- `compare/point_get/lin`: 195.4 ns/op; legacy 260.0, ratio 0.75x; historical only: OS/architecture mismatch
  - sqlite: win 3.61x (706.4 ns/op)
  - duckdb: win 1647.57x (321967.8 ns/op)
- `compare/filter_eq/lin`: 1335.2 ns/op; legacy 1040.0, ratio 1.28x; historical only: OS/architecture mismatch
  - sqlite: win 113.86x (152030.9 ns/op)
  - duckdb: win 233.10x (311239.0 ns/op)
- `compare/join_inner/lin`: 526343.2 ns/op; legacy 153500.0, ratio 3.43x; historical only: OS/architecture mismatch
  - sqlite: win 8.75x (4604087.0 ns/op)
  - duckdb: win 5.18x (2726478.5 ns/op)
