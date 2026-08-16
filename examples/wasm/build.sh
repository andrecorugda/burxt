#!/usr/bin/env bash
# examples/wasm/build.sh — a Burxt program, in a browser engine, in three commands.
#
#     ./build.sh
#
# Everything this needs is already on a machine that can build Burxt. There is no
# emscripten, no wasi-sdk, no sysroot and no `apt install`.
#
# Two islands are built, and the difference between them IS the documentation: one takes
# `String`s and needs no integer formatting, one carries a `Decimal<2>` and needs `snprintf`.
# Each is rendered natively and in wasm, and the two are diffed — see the note about
# zero-padding in `money-island.bx`, which is why the diff is not decoration.
set -euo pipefail
cd "$(dirname "$0")"

BURXT="${BURXT:-../../target/release/burxt}"
OUT="${OUT:-$(mktemp -d)}"

# **`wasm-ld` without installing a linker.** `lld` is usually not present, but the Rust
# toolchain ships one, and `rust-lld -flavor wasm` IS `wasm-ld`. This is the single fact that
# turns "wasm host glue" from a toolchain project into an afternoon.
LLD="${LLD:-$(echo "$HOME"/.rustup/toolchains/*/lib/rustlib/*/bin/rust-lld | cut -d' ' -f1)}"
if [ ! -x "$LLD" ]; then
    echo "no rust-lld found; set LLD= to a wasm-ld" >&2
    exit 1
fi

# `--target` emits an OBJECT and stops, exactly as for any other foreign triple. Then the link
# line, and every flag on it is load-bearing:
#
#   --no-entry          there is no `_start`; the host calls `main` and `bx.island` itself
#   --allow-undefined   NOT `--import-undefined`. `stderr` is a DATA symbol, and
#                       `--import-undefined` only turns undefined FUNCTIONS into imports,
#                       so the link fails on `stderr` with three identical errors
#   --export            wasm-ld exports nothing by default; name what the host will call
#   -z stack-size       the number `getrlimit` reports back in host.mjs. They must agree
#   --max-memory        without it the memory is not growable and `malloc` cannot serve the
#                       region at all
build_island() {
    local src="$1" stem="${1%.bx}"
    "$BURXT" build "$src" --target wasm32-unknown-unknown -o "$OUT/$stem.o" >/dev/null
    "$LLD" -flavor wasm \
        --no-entry --allow-undefined \
        --export=main --export='bx.island' \
        -z stack-size=1048576 --initial-memory=4194304 --max-memory=268435456 \
        "$OUT/$stem.o" -o "$OUT/$stem.wasm"
}

compare() {
    local src="$1" stem="${1%.bx}"
    shift
    echo "── $stem ────────────────────────────────────────────────────────"

    build_island "$src"
    echo "asks the host for:"
    llvm-nm-18 -u "$OUT/$stem.o" 2>/dev/null | grep -v '__' | tr -s ' ' | sed 's/^ *U /  /' \
        || echo "  (llvm-nm-18 not found; skipping the symbol list)"

    "$BURXT" run "$src" > "$OUT/$stem.native"
    node host.mjs "$OUT/$stem.wasm" "$@" | tail -1 > "$OUT/$stem.wasm.out"

    echo
    echo "native: $(cat "$OUT/$stem.native")"
    echo "wasm:   $(cat "$OUT/$stem.wasm.out")"
    if diff -q "$OUT/$stem.native" "$OUT/$stem.wasm.out" >/dev/null; then
        echo "        ✓ identical"
    else
        echo "        ✗ THEY DIFFER — see the zero-padding note in money-island.bx" >&2
        return 1
    fi
    echo
}

compare island.bx 'Ada Lovelace' 'One <script> compiler' '$1,299.00'
compare money-island.bx 'Grace Hopper' 129905n

echo "Both wasm renders came out of a WebAssembly engine that has never heard of Burxt,"
echo "and the escaped '<' in the first one was escaped by Burxt, not by the host."
