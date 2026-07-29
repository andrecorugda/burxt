#!/usr/bin/env bash
# Put a working `burxt` on PATH, and the editor extension in the editor.
#
# Two paths, and the first one is the one that proves something. A published release is a single
# static binary that needs no Rust and no LLVM, so installing it in a bare container is the same
# claim the install page makes, tested. Building from source is the fallback for a clone with no
# release yet — correct, but it installs a toolchain and takes minutes, so it announces itself.
set -euo pipefail
cd "$(dirname "$0")/.."

REPO="andrecorugda/burxt"
DEST="$HOME/.local"
mkdir -p "$DEST/bin" "$DEST/lib/burxt"

say() { printf '\n\033[1m==> %s\033[0m\n' "$*"; }

installed_from_release=0

say "Looking for a published Burxt release"
URL="$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" 2>/dev/null \
        | grep -o '"browser_download_url": *"[^"]*linux-x86_64\.tar\.gz"' \
        | head -1 | cut -d'"' -f4 || true)"

if [ -n "${URL:-}" ]; then
    say "Installing the release binary — no Rust, no LLVM, that is the whole point"
    echo "    $URL"
    WORK="$(mktemp -d)"
    if curl -fsSL "$URL" -o "$WORK/burxt.tar.gz"; then
        tar xzf "$WORK/burxt.tar.gz" -C "$WORK"
        UNPACKED="$(find "$WORK" -maxdepth 1 -type d -name 'burxt-*' | head -1)"
        if [ -n "$UNPACKED" ] && [ -x "$UNPACKED/burxt" ]; then
            install -m 755 "$UNPACKED/burxt" "$DEST/bin/burxt"
            install -m 644 "$UNPACKED"/lib/*.bx "$DEST/lib/burxt/" 2>/dev/null || true
            installed_from_release=1
        fi
    fi
    rm -rf "$WORK"
fi

if [ "$installed_from_release" = "0" ]; then
    say "No release yet — building from source instead"
    echo "    This installs Rust and LLVM 18 and takes a few minutes. It is not stuck."
    echo "    Once a release is tagged this step becomes a single download."
    sudo apt-get update -qq
    wget -q https://apt.llvm.org/llvm.sh -O /tmp/llvm.sh
    chmod +x /tmp/llvm.sh
    sudo /tmp/llvm.sh 18
    sudo apt-get install -y -qq llvm-18 llvm-18-dev libpolly-18-dev
    if ! command -v cargo >/dev/null 2>&1; then
        curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal
        # shellcheck disable=SC1091
        . "$HOME/.cargo/env"
    fi
    cargo build --release --locked
    install -m 755 target/release/burxt "$DEST/bin/burxt"
    install -m 644 lib/*.bx "$DEST/lib/burxt/"
fi

# PATH for every future shell in this container.
if ! grep -qs 'HOME/.local/bin' "$HOME/.bashrc"; then
    echo 'export PATH="$HOME/.local/bin:$PATH"' >> "$HOME/.bashrc"
fi
export PATH="$DEST/bin:$PATH"

# The extension, from the repository rather than a marketplace — it is not published, and installing
# the checked-in .vsix means this works on any branch instead of only after a release. `|| true`
# because a missing `code` CLI should not fail the container; the compiler still works without it.
say "Installing the editor extension"
VSIX="$(ls editors/vscode/burxt-*.vsix 2>/dev/null | head -1 || true)"
if [ -n "$VSIX" ] && command -v code >/dev/null 2>&1; then
    code --install-extension "$VSIX" 2>&1 | tail -1 || true
else
    echo "    skipped (no .vsix or no code CLI) — run: python3 editors/vscode/pack.py"
fi

# Prove it, rather than claiming it. If this fails the container is broken and the log says how.
say "Checking it works"
cd /tmp
printf 'print("Hello, world!");\n' > burxt_check.bx
if "$DEST/bin/burxt" run burxt_check.bx | grep -qx 'Hello, world!'; then
    echo "    a one-line program is a whole program — no entry point to declare"
else
    echo "    FAILED: burxt could not compile and run a one-line program" >&2
    exit 1
fi
rm -f burxt_check.bx burxt_check
cd - >/dev/null

cat <<'TXT'

  Burxt is ready.

    burxt run examples/tour.bx        most of the language in one file
    burxt run examples/money.bx       why exact decimals are the point
    burxt check <file.bx>             typecheck only, needs no linker

  The guide is in docs/guide/ — eleven pages in reading order.
  Open a .bx file and the extension gives you highlighting and live diagnostics.

TXT
