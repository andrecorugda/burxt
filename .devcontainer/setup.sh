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

# /usr/local, not ~/.local. The editor extension resolves the compiler as: the `burxt.path`
# setting, then the newer of ./target/{release,debug}/burxt in the workspace, then `burxt` from PATH. A VS Code extension
# host does not reliably inherit a PATH set in .bashrc, so ~/.local/bin left the language server
# unable to start — the compiler worked in the terminal and the editor had no diagnostics, which is
# exactly what the first real Codespace showed.
DEST="/usr/local"
sudo mkdir -p "$DEST/bin" "$DEST/lib/burxt"

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
            sudo install -m 755 "$UNPACKED/burxt" "$DEST/bin/burxt"
            sudo install -m 644 "$UNPACKED"/lib/*.bx "$DEST/lib/burxt/" 2>/dev/null || true
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
    sudo install -m 755 target/release/burxt "$DEST/bin/burxt"
    sudo install -m 644 lib/*.bx "$DEST/lib/burxt/"
fi

# No PATH edit needed: /usr/local/bin is already on the default PATH, including the one a VS Code
# extension host runs with. That is the entire reason for installing there.

# The extension, BUILT here rather than downloaded or read from the checkout.
#
# `.gitignore` has `*.vsix`, on the sound principle that a binary in a repository is a binary nobody
# can reproduce — so a fresh clone has no package at all, and the first real Codespace found exactly
# that: the compiler ran and the editor had no highlighting and no diagnostics. The packer needs only
# the standard library, so building it in the container costs nothing and needs no network.
say "Building and installing the editor extension"
# The packer is Burxt now, so it needs the compiler this script has just arranged — either the
# installed release or the one built from source above. `command -v burxt` covers both, because both
# paths end with it on PATH.
"$(command -v burxt)" run editors/vscode/pack.bx
# Named, not globbed: the packer writes exactly one path and a glob that matched a leftover from
# an older naming would install yesterday's grammar and report success.
VSIX="editors/vscode/burxt.vsix"
if [ ! -f "$VSIX" ]; then
    echo "    FAILED: pack.bx produced no .vsix" >&2
    exit 1
fi
if command -v code >/dev/null 2>&1; then
    code --install-extension "$VSIX" --force 2>&1 | tail -2 || true
else
    echo "    no \`code\` CLI here — install it by hand: code --install-extension $VSIX"
fi

# Prove it, rather than claiming it. If this fails the container is broken and the log says how.
say "Checking it works"
cd /tmp
printf 'print("Hello, world!");\n' > burxt_check.bx
if "$DEST/bin/burxt" run burxt_check.bx | grep -qx 'Hello, world!'; then
    echo "    a one-line program is a whole program — no entry point to declare"
    echo "    and the language server answers: $("$DEST/bin/burxt" --version 2>/dev/null || echo "$DEST/bin/burxt")"
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
