---
layout: default
title: Install
section: install
description: Burxt needs a C compiler and nothing else. Programs it compiles need libc.
---

# Install

**Four platforms**, built and tested by the same workflow that publishes them: Linux and macOS, on
Intel and on ARM. Windows runs through a container — there is no `burxt.exe`, deliberately, and the
reason is below.

## One line, on any of the four

It works out which tarball you need from `uname`, which is exactly how the tarball was named:

```sh
V={{ site.burxt_version }}
sh scripts/install.sh \
  https://github.com/andrecorugda/burxt/releases/download/v$V/burxt-$V-$(uname -s | tr 'A-Z' 'a-z')-$(uname -m).tar.gz
```

Then:

```sh
burxt run examples/tour.bx
```

`PREFIX=~/.local sh scripts/install.sh ...` puts it somewhere other than `/usr/local`.

## Or pick your own

<div class="tablewrap" markdown="1">

| Your machine | `uname -s`/`-m` | The asset |
|---|---|---|
| Linux, Intel or AMD | `Linux` / `x86_64` | `burxt-{{ site.burxt_version }}-linux-x86_64.tar.gz` |
| Linux, ARM | `Linux` / `aarch64` | `burxt-{{ site.burxt_version }}-linux-aarch64.tar.gz` |
| macOS, Apple silicon | `Darwin` / `arm64` | `burxt-{{ site.burxt_version }}-darwin-arm64.tar.gz` |
| macOS, Intel | `Darwin` / `x86_64` | `burxt-{{ site.burxt_version }}-darwin-x86_64.tar.gz` |

</div>

Linux says `aarch64` where macOS says `arm64` for the same processor. The names here follow
`uname` rather than tidying it, because the command above is `uname` and the two must agree.

Every release also carries **`SHA256SUMS`** and the **VS Code extension** as a `.vsix`. All of it is
on [the releases page](https://github.com/andrecorugda/burxt/releases).

```sh
tar xzf burxt-*-*.tar.gz
cd burxt-*
cp burxt ~/.local/bin/
```

## What the tarball needs from your Linux

**glibc 2.39 or newer**, which means **Ubuntu 24.04+, Debian 13+, Fedora 40+, RHEL 10+**.

The Linux binaries are built on Ubuntu 24.04 and link that glibc, so an older distribution answers
`version GLIBC_2.39 not found` and stops. That is not a helpful message and it is the first thing a
reader would meet, so it is written here instead.

On an older Linux, use the container below — it is the same binary on a base image that carries the
right glibc. macOS has no equivalent floor: those binaries link only what the system ships.

## Windows

There is no native build and that is a decision, not a gap: it would mean an MSVC toolchain in the
matrix and a linker this project does not test. The container is the supported route and it is the
same binary Linux gets.

```sh
wslc  run --rm -v "$PWD:/work" ghcr.io/andrecorugda/burxt run hello.bx   # Windows 11
docker run --rm -v "$PWD:/work" ghcr.io/andrecorugda/burxt run hello.bx  # anywhere
```

`wslc` is Windows 11's built-in Linux container runtime — no Docker Desktop and no third-party
runtime to install.

## What it needs from your machine

**A C compiler**, for `burxt build` to link with — `build-essential` on Debian or Ubuntu,
`xcode-select --install` on macOS. `burxt check` typechecks without linking and needs none of it.

**And on Linux, five shared libraries.** This page said *"That is the whole list"* about the C
compiler, and it was not:

```
libffi.so.8   libstdc++.so.6   libtinfo.so.6   libz.so.1   libzstd.so.1
```

A desktop distribution has all five already. A minimal container or a trimmed server image may not
— which is exactly how this was found, when the official image moved to a base where `libffi8` was
no longer pulled in by accident and the binary stopped starting. On Debian or Ubuntu:
`apt-get install libffi8 libtinfo6 zlib1g libzstd1`.

**macOS needs nothing beyond the system.** Those binaries link only what Apple ships.

`burxt build` produces an object file and hands it to the system linker, so `cc` has to exist.

**No Rust. No LLVM. No version to match.** LLVM is statically linked into the binary, which is why
it is 48 MB. That is a deliberate trade: one download now, rather than a system dependency that has
to keep matching forever. A dynamically linked build would be about 2 MB and would greet many people
with `error while loading shared libraries`, which is the commonest first-run failure for languages
that go the other way.

## What the programs you write need

**libc.** A compiled Burxt program is a native executable of about **16 KB** with nothing behind it:

```
linux-vdso.so.1    libc.so.6    ld-linux-x86-64.so.2
```

The bump allocator, the string operations and the overflow checks are emitted into every module.
There is no runtime to install, no VM to start, and nothing to keep in step with the compiler.

## The sizes, measured

<div class="tablewrap" markdown="1">

| | |
|---|---|
| The tarball | ~18 MB compressed |
| The binary, stripped | 48 MB — almost all of it LLVM |
| A compiled program | ~16 KB |

</div>

## Without installing anything

[Open it in a Codespace](https://codespaces.new/andrecorugda/burxt?quickstart=1) — a browser, the
real compiler, and the editor extension with live diagnostics on real code.

## Building from source

Only needed to work *on* the compiler. This is the one path that wants a toolchain:

```sh
git clone https://github.com/andrecorugda/burxt
cd burxt
cargo build --release            # needs Rust and LLVM 18
cargo test                       # 34 invariants, including the byte-identical fixpoint
```

LLVM **18** exactly — the binding is feature-gated to it, so a newer one will not link.
`.cargo/config.toml` points at `/usr/lib/llvm-18`, which is where `apt.llvm.org`'s script puts it.

`sh scripts/release.sh` builds the tarball and then unpacks its own output into a scratch directory
and compiles a program with the *unpacked* binary before reporting success — so a broken artifact
fails at build time rather than on somebody else's machine.

## Editor support

The tarball ships the compiler; the extension is a separate asset on the same release. It gives
syntax highlighting and live diagnostics from the compiler itself, not a reimplementation of its
rules:

```sh
code --install-extension burxt-*.vsix
```
