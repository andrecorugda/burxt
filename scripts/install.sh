#!/bin/sh
# Install Burxt from a release tarball.
#
#     sh scripts/install.sh                        # from ./dist, built locally
#     sh scripts/install.sh <path-or-url.tar.gz>   # from a tarball you have or can fetch
#     PREFIX=~/.local sh scripts/install.sh        # somewhere other than /usr/local
#
# What it installs: the `burxt` binary, and the standard library where `use` can find it.
# What it needs from you: a C compiler, because `burxt build` calls `cc` to link. LLVM is
# inside the binary; Rust and cargo are not needed at all.
set -e

PREFIX="${PREFIX:-/usr/local}"
SOURCE="$1"
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

if [ -z "$SOURCE" ]; then
    SOURCE=$(ls -t dist/burxt-*.tar.gz 2>/dev/null | head -1 || true)
    if [ -z "$SOURCE" ]; then
        echo "no tarball given and none in dist/ — run: sh scripts/release.sh" >&2
        exit 1
    fi
fi

case "$SOURCE" in
    http*://*)
        echo "fetching $SOURCE"
        curl -fsSL "$SOURCE" -o "$WORK/burxt.tar.gz"
        SOURCE="$WORK/burxt.tar.gz"
        ;;
esac

echo "unpacking $SOURCE"
tar xzf "$SOURCE" -C "$WORK"
UNPACKED=$(find "$WORK" -maxdepth 1 -type d -name 'burxt-*' | head -1)
if [ -z "$UNPACKED" ]; then
    echo "that tarball does not look like a Burxt release" >&2
    exit 1
fi

# A C compiler is the one thing the binary cannot carry: say so before installing, not
# when the first build fails.
if ! command -v cc >/dev/null 2>&1; then
    echo "warning: no \`cc\` on PATH. \`burxt check\` will work; \`burxt build\` and" >&2
    echo "         \`burxt run\` need a C compiler to link. Install one (build-essential," >&2
    echo "         xcode-select --install) and they will work." >&2
fi

install -d "$PREFIX/bin" "$PREFIX/lib/burxt"
install -m 755 "$UNPACKED/burxt" "$PREFIX/bin/burxt"
install -m 644 "$UNPACKED"/lib/*.bx "$PREFIX/lib/burxt/"
[ -f "$UNPACKED/lib/README.md" ] && install -m 644 "$UNPACKED/lib/README.md" "$PREFIX/lib/burxt/"

echo
echo "installed $("$PREFIX/bin/burxt" 2>&1 | head -1)"
echo "  binary:  $PREFIX/bin/burxt"
echo "  library: $PREFIX/lib/burxt/    (use \"$PREFIX/lib/burxt/string.bx\";)"
case ":$PATH:" in
    *":$PREFIX/bin:"*) ;;
    *) echo "  note:    $PREFIX/bin is not on your PATH" ;;
esac
