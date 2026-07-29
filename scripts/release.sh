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
strip -o "$OUT/burxt" target/release/burxt
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
- Linux x86-64 for this tarball. Other targets are a build away — the compiler's front end
  knows nothing about platforms — but they are not built here yet.

## Where things are

- \`lib/\` — the standard library, written in Burxt: strings, files, the machine.
  \`use "/usr/local/lib/burxt/str.bx";\`
- The guide, the examples and the source: https://github.com/andrecorugda/burxt
EOF

tar -czf "dist/$NAME.tar.gz" -C dist "$NAME"
rm -rf "$OUT"
echo "wrote dist/$NAME.tar.gz ($(du -h "dist/$NAME.tar.gz" | cut -f1))"
