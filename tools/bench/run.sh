#!/bin/bash
set -euo pipefail
pages=${BENCH_PAGES:-$HOME/.cache/bande-web/pages}
root=$(git -C "$(dirname "$0")" rev-parse --show-toplevel)
mode=${1:-compare}
shift || true
rm -rf "$root/.bench-pages"
cp -al "$pages" "$root/.bench-pages" 2>/dev/null || cp -a "$pages" "$root/.bench-pages"
list=$(mktemp)
host=${BENCH_HOST:-bande-builder}
echo "$host" > "$list"
flock "$HOME/bande-linux/builders.d/slots/$host.0" true
trap 'rm -rf "$list" "$root/.bench-pages"' EXIT
BUILDERS_FILE=$list BUILD_NO_MINI=1 ~/bande-linux/bin/build-run "$root" bash -c '
set -euo pipefail
cargo build -q --profile perf -p web-bench -p web-renderer -p web-netd
exec unshare -Un --map-current-user --keep-caps bash -c "ip link set lo up; exec target/perf/bench \"\$@\"" bench "$@"
' run "$mode" "$@"
