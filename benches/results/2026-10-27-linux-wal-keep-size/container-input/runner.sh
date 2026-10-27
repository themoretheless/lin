set -eu
mkdir /work
cp -R /input/. /work/
cd /work
rustc -Vv > /results/rustc.txt
uname -srmo > /results/kernel.txt
stat -f -c %T /work > /results/filesystem.txt
cargo test --release --offline -- --nocapture > /results/tests.log 2>&1
cp Cargo.lock /results/Cargo.lock
sed -i 's/libc::FALLOC_FL_KEEP_SIZE/0/' src/helper.rs
set +e
cargo test --release --offline native_fs::keep_size_preserves_cursor_and_bytes -- --nocapture > /results/mutant.log 2>&1
result=$?
set -e
printf '%s\n' "$result" > /results/mutant-exit-code.txt
test "$result" -ne 0
