#!/usr/bin/env bash
# Build the CLI and qualification worker with the same allocator on native Linux.
set -euo pipefail
args=(--locked --release)
link=()
case "$(rustc -vV | sed -n 's/^host: //p')" in
    x86_64-unknown-linux-gnu)
        archive=${LIN_TCMALLOC_ARCHIVE:-$(c++ -print-file-name=libtcmalloc_minimal.a)}
        test -f "$archive" || { echo 'Install the gperftools development package for the static allocator archive.' >&2; exit 1; }
        link=(-C link-arg=-Wl,-u,tc_malloc -C "link-arg=$archive" -C link-arg=-lstdc++)
        ;;
esac
for binary in lin lin-peer-worker; do
    cargo rustc "${args[@]}" -p "$binary" --bin "$binary" -- "${link[@]}"
done
