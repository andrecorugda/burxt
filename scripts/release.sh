#!/bin/sh
# Build a release tarball: one binary, the standard library, and the guide.
#
#     sh scripts/release.sh            # writes dist/burxt-<version>-linux-x86_64.tar.gz
#
# The binary statically links LLVM, so whoever installs it needs no LLVM, no Rust and no
# cargo — only a C compiler, because `burxt build` calls `cc` to link. That is stated in
# the tarball's README rather than discovered.
set -e
cd "$(dirname "$0")/.."

VERSION=$(grep '^version = ' Cargo.toml | head -1 | cut -d'"' -f2)
TARGET=$(uname -s | tr 'A-Z' 'a-z')-$(uname -m)
NAME="burxt-$VERSION-$TARGET"
OUT="dist/$NAME"

echo "building burxt $VERSION for $TARGET"
export LLVM_SYS_181_PREFIX="${LLVM_SYS_181_PREFIX:-/usr/lib/llvm-18}"
cargo build --release

rm -rf "$OUT" && mkdir -p "$OUT/lib"
# Copy then strip in place, rather than `strip -o dest src`. GNU and BSD strip disagree about
# `-o` in ways that are not worth discovering on a release runner, and copy-then-strip is the
# spelling both agree on.
cp target/release/burxt "$OUT/burxt"
strip "$OUT/burxt"
cp lib/*.bx "$OUT/lib/"
cp lib/README.md "$OUT/lib/"
cp LICENSE-MIT LICENSE-APACHE "$OUT/"

cat > "$OUT/README.md" <<EOF
# Burxt $VERSION — $TARGET

A typed, compiled, native language where exact decimals are the default.

## Install

    sudo cp burxt /usr/local/bin/
    sudo mkdir -p /usr/local/lib/burxt && sudo cp -r lib/* /usr/local/lib/burxt/

Or keep it wherever you unpacked it and call it by path.

## Run something

    cat > hello.bx <<'BX'
    let price: Decimal<2> = 19.99;
    print(price * 3);
    BX
    burxt run hello.bx        # 59.97, exactly

## What this needs from your machine

- **A C compiler** (\`cc\`), because \`burxt build\` hands the object file to the system
  linker. Nothing else: LLVM is inside this binary.
- **$TARGET** — this tarball runs on that and nothing else. Other hosts are published
  alongside it: Linux x86-64, Linux arm64, macOS arm64 and macOS x86-64.

  On **Windows**, use the container image rather than this tarball:

      docker run --rm -v "\$PWD:/work" ghcr.io/andrecorugda/burxt run hello.bx
      wslc   run --rm -v "\$PWD:/work" ghcr.io/andrecorugda/burxt run hello.bx

  \`wslc\` is Windows 11's built-in Linux container runtime — no Docker Desktop needed.

## Compiling FOR another machine

The host above is where the *compiler* runs. What it can *emit* is a longer list, and the
LLVM IR is byte-identical for every one of them, which is what makes the decimal answers
identical too:

    burxt build pay.bx --target aarch64-linux-android -o pay.o

That writes an **object and stops**, on purpose: linking needs that platform's libc, sysroot
and linker, so it is handed to the toolchain that already has them (the NDK here). Verified
to emit: \`aarch64-unknown-linux-gnu\`, \`x86_64-unknown-linux-gnu\`, \`riscv64-unknown-linux-gnu\`,
\`aarch64-apple-darwin\`, \`x86_64-apple-darwin\`, \`x86_64-pc-windows-msvc\`,
\`armv7-unknown-linux-gnueabihf\`, \`wasm32-unknown-unknown\`, \`wasm32-wasi\`,
\`aarch64-apple-ios\`, and the three Android ABIs
(\`aarch64-linux-android\`, \`armv7a-linux-androideabi\`, \`x86_64-linux-android\`).

## Where things are

- \`lib/\` — the standard library, written in Burxt. \`option.bx\` and \`result.bx\` are how
  absence and failure work at all; \`map.bx\` is a key-value table in insertion order;
  \`string.bx\`, \`files.bx\` and \`os.bx\` are what their names say.
  \`use "/usr/local/lib/burxt/string.bx";\`
- The guide, the examples and the source: https://github.com/andrecorugda/burxt
EOF

tar -czf "dist/$NAME.tar.gz" -C dist "$NAME"
rm -rf "$OUT"

# ---- the smoke test, against the TARBALL ---------------------------------------------------------
# The repository being green says nothing about the artifact. This unpacks what a stranger
# downloads, into a directory that has never seen this project, and uses the UNPACKED binary.
#
# Added after the release script had shipped instructions for a `lib/str.bx` that does not exist —
# the file is `string.bx`. Nothing caught it because nothing ever opened the tarball.
SMOKE=$(mktemp -d)
trap 'rm -rf "$SMOKE"' EXIT
tar xzf "dist/$NAME.tar.gz" -C "$SMOKE"
BIN="$SMOKE/$NAME/burxt"

[ -x "$BIN" ] || { echo "FAIL: no executable in the tarball" >&2; exit 1; }

# If this links libLLVM then "needs no LLVM installed" is false, and that sentence is on the
# install page.
#
# The tool differs by platform, and getting this wrong is silent rather than loud: `ldd` does
# not exist on macOS, so the original `ldd | grep` simply found nothing there and the check
# PASSED for every Darwin build without ever looking. A guard that cannot fail is not a guard
# — so the tool is chosen explicitly and an unknown platform is a hard stop, not a shrug.
case "$(uname -s)" in
    Linux)  SHARED_LIBS="ldd $BIN" ;;
    Darwin) SHARED_LIBS="otool -L $BIN" ;;
    *)      echo "FAIL: no way to list shared libraries on $(uname -s)" >&2; exit 1 ;;
esac
if ! $SHARED_LIBS >/dev/null 2>&1; then
    echo "FAIL: could not inspect the binary's shared libraries with: $SHARED_LIBS" >&2
    exit 1
fi
if $SHARED_LIBS 2>/dev/null | grep -qi llvm; then
    echo "FAIL: the binary links libLLVM, so it is not standalone" >&2
    $SHARED_LIBS | grep -i llvm >&2
    exit 1
fi

cd "$SMOKE/$NAME"
printf 'print("Hello, world!");\n' > hello.bx
"$BIN" run hello.bx > out.txt 2>&1 || { echo "FAIL: cannot run a one-line program" >&2; cat out.txt >&2; exit 1; }
grep -qx 'Hello, world!' out.txt || { echo "FAIL: wrong output for hello" >&2; cat out.txt >&2; exit 1; }

# Every library file the README names must actually be there and actually load. This is the check
# that would have caught `str.bx`.
for f in option result map string files os; do
    [ -f "lib/$f.bx" ] || { echo "FAIL: lib/$f.bx is named in the README and missing" >&2; exit 1; }
done
cat > lib_check.bx <<'BX'
use "lib/option.bx";
use "lib/map.bx";
region r {
    let mutable counts: Map<String, Int> = map_new();
    let put: Int = counts.set("apples", 3);
    print(option_or(counts.find("apples"), 0));
}
BX
"$BIN" run lib_check.bx > lib.txt 2>&1 || { echo "FAIL: the library does not work from the tarball" >&2; cat lib.txt >&2; exit 1; }
grep -qx '3' lib.txt || { echo "FAIL: wrong output from the library" >&2; cat lib.txt >&2; exit 1; }
cd - >/dev/null

echo "wrote dist/$NAME.tar.gz ($(du -h "dist/$NAME.tar.gz" | cut -f1)) — smoke test passed"
echo "sha256 $(sha256sum "dist/$NAME.tar.gz" | cut -d' ' -f1)"
