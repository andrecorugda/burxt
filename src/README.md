# `src/` — two compilers, and which one is the product

Both directories here are **source**, which is why both are in `src/`. They are not peers.

| | | |
|---|---|---|
| **`burxt-compiler/`** | **Burxt** | **The product.** 16,000 lines. It compiles every program the other one does, compiles itself to a byte-identical fixpoint, and carries the CLI a user types. |
| `rust-compiler/` | Rust | The **bootstrap** and the **oracle**. Not the product, and not a peer. |

## Why the Rust one is not under `tests/`

Andre asked, and it is the right question — it is not main code, and it is what cross-checks the real
compiler, so `tests/` is where its *status* belongs. The answer is that it has **two jobs, and only one
of them is a test**:

1. **It is the bootstrap.** On a machine with no Burxt binary, `cargo build` is the only way to get
   one. `main.bx` cannot compile itself from nothing; something has to compile it first. That job is
   not optional and it is not a test, and a directory called `tests/` would say otherwise — a
   contributor could reasonably skip building "just the tests" and then be unable to build Burxt at
   all.
2. **It is the differential oracle**, and that half genuinely is test-shaped: two implementations that
   must agree turn a language change into a failing test instead of a bug report.

The first job is what pins it here. `Cargo.toml` points `[[bin]]` at `src/rust-compiler/main.rs`, and
that is the entry point for anyone building from source.

**So the honest fix was not to move it but to say what it is** — which is this file, because the
directory listing could not say it on its own. `spec/ROADMAP-1.0.md` §THE GATE has the rule the two
follow: **Rust may BUILD Burxt; Burxt may not USE Rust.** A bootstrap is a one-time debt. A dependency
is permanent.

## What "the two agree" means

Not identical text. Andre's ruling:

> *"When I say equal it doesn't mean identical literal. I said it basing on the output/result. Burxt is
> not Rust and vice versa, so there will always be difference. As long as we can give the same result
> in the Burxt way, that is a yes."*

`every_rust_module_has_a_burxt_counterpart_or_a_reason` in `tests/runner.rs` holds every `.rs` file
here to a named counterpart and a named comparison. **10 of 11 rows are held**; `main.rs` is the last,
and it owes `--json` and `explain memory` rather than owing agreement.

Three times the Burxt implementation has audited the Rust one, and each time because it did the job
differently rather than identically: `diag.bx` is total where `diag.rs` **panicked** on a span ending
mid-character; `lsp.bx` answers hover on files with `use` lines where `lsp.rs` answered **nothing at
all**; and `lsp.bx` scans for the key it wants instead of building a tree, so a malformed message is
*absent* rather than a *parse error*. A transliteration would have inherited all three bugs.
