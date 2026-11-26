#!/usr/bin/env bash
# Run the live Azure comparison on the x64 host that owns the credentials.
set -euo pipefail
umask 077
if [[ $(uname -sm) != 'Linux x86_64' ]]; then
    echo 'The linked qualification requires native x86_64 Linux.' >&2
    exit 1
fi
: "${LIN_BENCH_KUSTO_URL:?Set the HTTPS endpoint of the dedicated Azure test cluster}"
: "${LIN_BENCH_KUSTO_DATABASE:?Set the dedicated benchmark database}"
[[ "$LIN_BENCH_KUSTO_URL" == https://* ]] || { echo 'Expected an HTTPS endpoint.' >&2; exit 1; }
output=$(realpath -m "${1:?Specify a new private result directory}")
mkdir "$output"
cd "$(dirname "$0")/.."
scripts/build-linux-release.sh
worker="${CARGO_TARGET_DIR:-target}/release/lin-peer-worker"
nm --defined-only "$worker" > "$output/symbols.txt"
grep -Eq ' [TW] tc_malloc$' "$output/symbols.txt"
grep -Eq ' [TW] malloc$' "$output/symbols.txt"
sha256sum "$worker" > "$output/binary.sha256"
lscpu > "$output/cpu.txt"
export LIN_BENCH_KUSTO_INGEST_MODE=inline
for rows in 1000 10000 100000; do
    for mode in read insert; do
        args=()
        if [[ "$mode" == insert ]]; then args=(--cases insert_native); fi
        token=${LIN_BENCH_KUSTO_TOKEN:-}
        if [[ -z "$token" ]]; then
            token=$(az account get-access-token --resource https://kusto.kusto.windows.net --query accessToken -o tsv)
        fi
        LIN_BENCH_KUSTO_TOKEN="$token" python3 scripts/bench-peers.py --lin-binary "$worker" \
            --engines lin kusto --rows "$rows" --samples 3 --repeats 3 "${args[@]}" \
            --output "$output/$mode-$rows"
    done
done
sha256sum -c "$output/binary.sha256"
