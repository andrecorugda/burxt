# Burxt for VS Code

Syntax highlighting, live diagnostics and hover for [Burxt](https://github.com/andrecorugda/burxt)
— a typed, compiled language where exact decimals are the default and correctness is
enforced by the compiler.

- **Highlighting** that knows the language: `$19.99` and `8.25%` as numeric literals,
  `Decimal<2, RoundHalfEven>` with the scale and the rounding contract distinct,
  `{interpolation}` as embedded code, and a bare `}` in a string flagged invalid
  because the compiler flags it too.
- **Errors as you type**, from the compiler itself — not a second implementation of
  its rules that can disagree with it.
- **Hover** showing a value's exact type *and what that type guarantees*: a
  `Decimal<2, RoundHalfEven>` hover says results round half to even.

No `npm install` and no bundled runtime: the extension talks to `burxt lsp` over
stdio, the same language server every other editor uses.

## Requirements

The `burxt` compiler on `PATH`, or a workspace build at `./target/debug/burxt`, or
`burxt.path` set in settings.
