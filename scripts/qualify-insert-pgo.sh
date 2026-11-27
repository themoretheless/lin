#!/usr/bin/env bash
set -euo pipefail
scratch=$(mktemp -d)
mkdir "$scratch/build-profiles" "$scratch/training"
scripts/build-linux-release.sh
cp target/release/lin-peer-worker "$RUNNER_TEMP/thin-worker"
RUSTFLAGS="-Cprofile-generate=$scratch/build-profiles" scripts/build-linux-release.sh
for size in 768 4096 32768; do
  fixture=$(mktemp -d "$scratch/db.XXXXXX")
  printf '{"insert_rows":%d,"samples":1,"timestamp_ms":987654,"durable_dir":"%s"}\n' "$size" "$fixture" |
    LLVM_PROFILE_FILE="$scratch/training/disk-$size-%m.profraw" target/release/lin-peer-worker > /dev/null
  printf '{"insert_rows":%d,"samples":2,"timestamp_ms":987654}\n' "$size" |
    LLVM_PROFILE_FILE="$scratch/training/memory-$size-%m.profraw" target/release/lin-peer-worker > /dev/null
done
host=$(rustc -vV | sed -n 's/^host: //p')
profdata="$(rustc --print sysroot)/lib/rustlib/$host/bin/llvm-profdata"
"$profdata" merge -o "$scratch/training.profdata" "$scratch/training/"*.profraw
"$profdata" show "$scratch/training.profdata" > training-summary.txt
RUSTFLAGS="-Cprofile-use=$scratch/training.profdata" scripts/build-linux-release.sh
sha256sum "$RUNNER_TEMP/thin-worker" target/release/lin-peer-worker > lto-binaries.txt
