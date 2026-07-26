# Editor and platform support for Burxt

A language is not real to the people using it until their editor knows it. This
directory holds that half of the project.

| Piece | State | Where |
|---|---|---|
| TextMate grammar (highlighting) | **DONE** (v0.0.31) | `vscode/syntaxes/burxt.tmLanguage.json` |
| Language configuration (comments, brackets, indent) | **DONE** (v0.0.31) | `vscode/language-configuration.json` |
| VS Code extension | **DONE** (v0.0.31) — declarative, no build step | `vscode/` |
| `burxt check` — front end only, for editors and CI | **DONE** (v0.0.31) | `src/main.rs` |
| Diagnostics with line/column | **DONE** (v0.0.32) | `src/diag.rs`, `burxt check --json` |
| Language server (`burxt lsp`) | **DONE** (v0.0.33) — diagnostics on change | `src/lsp.rs` |
| VS Code problem matcher (squiggles without a client) | **DONE** (v0.0.33) | `vscode/package.json`, `.vscode/tasks.json` |
| Neovim / Helix configs | **DONE** (v0.0.33) — diagnostics, no highlighting yet | `nvim/`, `helix/` |
| Tree-sitter grammar (Neovim/Helix colour) | not written | see below |
| Hover, go-to-definition | not written | see below |
| GitHub language detection | blocked on a popularity gate, not on us | see below |

## Installing the VS Code extension

The extension is **declarative** — a grammar plus a language configuration, no
JavaScript and no build step — so it installs by being copied into place:

```bash
# VS Code
ln -s "$PWD/editors/vscode" ~/.vscode/extensions/burxt

# VS Code Insiders
ln -s "$PWD/editors/vscode" ~/.vscode-insiders/extensions/burxt

# VSCodium
ln -s "$PWD/editors/vscode" ~/.vscode-oss/extensions/burxt
```

Then reload the window. A `.bx` file should light up: money literals as numbers,
`Decimal<2, RoundHalfEven>` with the scale and the rounding contract distinct,
`{interpolation}` as embedded code, `\{` as an escape, and a bare `}` in a string
flagged as invalid — because the compiler refuses it too.

To package it as a `.vsix` instead (requires `npm i -g @vscode/vsce`):

```bash
cd editors/vscode && vsce package
```

## Other editors

The grammar is a standard TextMate grammar, which is the same artifact most
editors want:

- **Sublime Text / TextMate**: point at `burxt.tmLanguage.json` directly.
- **Zed**: TextMate grammars are supported via a language extension.
- **Neovim / Helix**: these want a **tree-sitter** grammar instead, which is a
  separate piece of work and not yet written. Recorded, not pretended.
- **JetBrains**: TextMate bundles are supported under
  *Settings → Editor → TextMate Bundles*.

## What is deliberately NOT here yet

**A tree-sitter grammar.** Neovim and Helix want one for *highlighting* — they
get diagnostics from the LSP today but no colour. It is the largest remaining gap
in editor support; writing two grammars that can disagree is also a real cost,
which is why the TextMate one came first (it serves VS Code, Sublime, Zed,
JetBrains, and Linguist from a single file).

**Formatting (`burxt fmt`).** A formatter is a design decision about the
language's canonical shape, not a tooling detail — it deserves its own spec
rather than being improvised.

## The language server

`burxt lsp` speaks the Language Server Protocol over stdio. It typechecks the
buffer you are editing — not the file on disk — and publishes one diagnostic or
none, because the compiler stops at the first error and pretending to a list would
be a lie. Publishing the empty list matters as much as publishing an error: it is
what clears the squiggle when the code becomes valid again.

```bash
cargo build && sudo ln -sf "$PWD/target/debug/burxt" /usr/local/bin/burxt
```

**Neovim** — `editors/nvim/burxt.lua`, no plugin manager and no `nvim-lspconfig`
needed:

```lua
vim.cmd('source /path/to/burxt/editors/nvim/burxt.lua')
```

**Helix** — append `editors/helix/languages.toml` to
`~/.config/helix/languages.toml`.

**Zed, Emacs (eglot/lsp-mode), Sublime LSP, Kate** — any LSP client works; the
command is `burxt lsp` and the file type is `.bx`.

**VS Code** — the extension is still declarative (no JavaScript), so it does not
yet *launch* the server. Until it does, squiggles come from a task and a problem
matcher, which needs no build step:

- `Ctrl+Shift+B` runs `burxt check` on the current file and populates the Problems
  panel. `.vscode/tasks.json` in this repo is the working example.
- The matcher is contributed as `$burxt` by the extension, so any task can use it.

Wiring the LSP into VS Code properly needs `vscode-languageclient`, which means
npm and a bundling step — a real cost against the extension's current property of
being copyable with no toolchain. It is the next piece here, not a missing one
elsewhere.

### What the server does NOT do yet

- **Hover** showing a value's exact type. `Decimal<2, RoundHalfEven>` on hover is
  worth more in Burxt than in most languages, since the rounding contract is part
  of the type — so this is the first thing worth adding.
- **Go-to-definition**, which needs the compiler to keep name resolution rather
  than only its result.
- **More than one error at a time**, which is a *compiler* change (error recovery),
  not a server change. Recorded here so the limitation is not mistaken for a
  server bug.
- **Incremental sync.** The server asks for full-document sync deliberately:
  applying incremental text edits correctly is fiddly, and a server that corrupts
  its own copy of the buffer reports errors about code you never wrote.

## GitHub language detection

`.bx` files currently show as plain text on github.com. Burxt is not in
[github/linguist](https://github.com/github-linguist/linguist) yet, and that is a
**popularity gate rather than a technical one**: Linguist asks that a language
already be in use across a few hundred public repositories before it is added.

What is already done, so the submission is a form-filling exercise when the bar
is met:

- [x] An open-source TextMate grammar (`vscode/syntaxes/burxt.tmLanguage.json`,
      scope `source.burxt`) — the artifact Linguist actually consumes.
- [x] Unambiguous extension: `.bx`.
- [x] Real sample programs to contribute as Linguist samples — `examples/` and
      the 230+ programs under `tests/`.
- [x] A licence Linguist accepts (MIT OR Apache-2.0).
- [ ] Usage across enough public repositories. **This is the only open item.**
- [ ] The `languages.yml` entry, submitted as a PR:

```yaml
Burxt:
  type: programming
  color: "#2f6f4f"
  extensions:
    - ".bx"
  tm_scope: source.burxt
  ace_mode: text
  language_id: <assigned by linguist>
```

**Deliberately not worked around.** `.gitattributes` could force `.bx` to be
counted as some other language and the repo would look "detected" — but it would
be *wrongly* detected, and a wrong label is worse than no label. That is the same
standard the language itself applies to a silently wrong number.
