#!/usr/bin/env bash
# Full check against the `boring` checkout: build boring, build the interpreter, run the parity
# tool on both corpora, then the three test suites. Prints the `boring` commit under test.
# Usage: tools/check.sh [--quick]    (--quick skips the slow variant-build suite, and with it the
# functional suite, which needs the four binaries that suite builds)
set -u
HERE="$(cd "$(dirname "$0")/.." && pwd)"
BORING_DIR="${BORING_DIR:-$HERE/../boring}"
OUT="${TMPDIR:-/tmp}/boring-self-interpreter-check"
mkdir -p "$OUT"
status=0
step() { echo; echo "== $*"; }

step "build boring ($BORING_DIR)"
(cd "$BORING_DIR" && cargo build --release -q) || exit 1

step "build the interpreter"
BORING="${BORING_BIN:-$BORING_DIR/target/release/boring}"
(cd "$HERE/interpreter" && "$BORING" build >"$OUT/build.log" 2>&1 \
  && cargo build -q --manifest-path main_rust/Cargo.toml) || { tail -20 "$OUT/build.log"; exit 1; }

step "parity, registered fixtures"
python3 "$HERE/tools/interpreter_parity.py" --timeout 6 --output "$OUT/parity.json" | tail -12
[ "${PIPESTATUS[0]}" -eq 0 ] || status=1

step "parity, unregistered fixtures (gaps listed in docs/interpreter-parity.md)"
python3 "$HERE/tools/interpreter_parity.py" --unregistered --timeout 6 --output "$OUT/parity-unregistered.json" | tail -12

suite() {
  local log="$OUT/$1.log"; shift
  (cd "$HERE" && cargo test "$@" >"$log" 2>&1) || status=1
  grep -E "^test result|FAILED" "$log"
}
step "suites"
suite interpreter --test interpreter
if [ "${1:-}" != "--quick" ]; then
  suite interpreter_build --test interpreter_build -- --test-threads=1
fi
if [ "${1:-}" != "--quick" ]; then
  suite interpreter_functional --test interpreter_functional
fi

echo; echo "boring under test: $(python3 -c "import json;print(json.load(open('$OUT/parity.json'))['boring'])")"
echo "results in $OUT"
exit $status
