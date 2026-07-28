# Editor and platform support for Burxt

A language is not real to the people using it until their editor knows it. This
directory holds that half of the project.

| Piece | State | Where |
|---|---|---|
| TextMate grammar (highlighting) | **DONE** (v0.0.31) | `vscode/syntaxes/burxt.tmLanguage.json` |
| Language configuration (comments, brackets, indent) | **DONE** (v0.0.31) | `vscode/language-configuration.json` |
| VS Code extension | **DONE** (v0.0.31), live diagnostics (v0.0.34) — still no build step | `vscode/` |
| `burxt check` — front end only, for editors and CI | **DONE** (v0.0.31) | `src/main.rs` |
| Diagnostics with line/column | **DONE** (v0.0.32) | `src/diag.rs`, `burxt check --json` |
| Language server (`burxt lsp`) | **DONE** (v0.0.33) — diagnostics on change, hover (v0.0.35) | `src/lsp.rs` |
| VS Code diagnostics + hover | **DONE** (v0.0.36) — hand-written LSP client, still no npm | `vscode/extension.js` |
| VS Code problem matcher (for tasks and CI) | **DONE** (v0.0.33) | `vscode/package.json`, `.vscode/tasks.json` |
| Neovim / Helix configs | **DONE** (v0.0.33) — diagnostics, no highlighting yet | `nvim/`, `helix/` |
| `.bx` file icon in the explorer | **DONE** (v0.0.42) — works under the default theme | `vscode/package.json` |
| Tree-sitter grammar (Neovim/Helix colour) | not written | see below |
| Hover, go-to-definition | not written | see below |
| GitHub language detection | blocked on a popularity gate, not on us | see below |

## Installing the VS Code extension

Package it and install it — no npm, no `vsce`, no bundler:

```bash
python3 editors/vscode/pack.py                                  # writes burxt-0.1.2.vsix
code --install-extension editors/vscode/burxt-0.1.2.vsix
```

`pack.py` is a .vsix writer in the standard library: a .vsix is a ZIP holding an OPC
content-types map, a VSIX manifest, and the extension under `extension/`. `vsce`
does more — linting, dependency bundling, marketplace checks — and all of it is for
publishing rather than installing, so none of it is needed here.

**Install rather than symlink.** A symlink into the extensions directory does work,
until something reads the extension registry and does not find you. An installed
extension is registered, versioned, upgradable and uninstallable through the normal
UI, and it is the shape every other extension has.

On a remote — WSL, SSH, a container — the manifest declares
`"extensionKind": ["workspace"]`, because the extension spawns the compiler and the
language server and therefore has to run where the code is rather than on the UI
side.

Then reload the window. A `.bx` file should light up: money literals as numbers,
`Decimal<2, RoundHalfEven>` with the scale and the rounding contract distinct,
`{interpolation}` as embedded code, `\{` as an escape, and a bare `}` in a string
flagged as invalid — because the compiler refuses it too.

To package it as a `.vsix` instead (requires `npm i -g @vscode/vsce`):

```bash
cd editors/vscode && vsce package
```

## Getting the Burxt icon on `.bx` files

`contributes.languages[].icon` is the whole mechanism — the same one
`apex-stack.apex-alpine` uses — and it works under the **default Seti theme**. No
icon theme to install, nothing to switch, nothing lost.

**A correction worth keeping, because v0.0.41 got this wrong.** I claimed Seti
ignores language-contributed icons and shipped a file icon theme to work around it.
That was an assumption, and it was false. VS Code's own logic is:

```js
n = true                     // set when a theme defines languageIds
showLanguageModeIcons === true || (n && showLanguageModeIcons !== false)
```

Seti defines 83 `languageIds` and never sets the flag to `false`, so the second
clause is true and language icons apply to any language Seti does not itself cover —
which includes Burxt. The icon theme has been removed; it solved a problem that did
not exist, at the cost of every other file's icon.

The lesson is the one this project applies to the compiler: **check the mechanism,
do not reason about it from memory.** The answer was in the shipped bundle the whole
time.

## Other editors
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

**VS Code** — errors as you type *and* hover, with **no `npm install`**. The
extension is a hand-written LSP client (about a hundred lines of framing and
request bookkeeping) rather than `vscode-languageclient`, because that package
would bring npm, a lock file and a bundling step, and the property worth
protecting is that this directory can be copied into place and used.

It talks to the same `burxt lsp` every other editor uses. That matters more than
the line count: there is one place where "what does the compiler know about this
buffer" is answered, and every editor asks it the same way.

Set `burxt.path` in settings if the compiler is not on `PATH` and not at
`./target/debug/burxt` in the workspace. *Burxt: Restart Language Server* is in the
command palette.

A `$burxt` problem matcher is also contributed, for tasks and CI: `Ctrl+Shift+B`
runs `burxt check` on the current file and fills the Problems panel;
`.vscode/tasks.json` here is the working example. `burxt check <file> --json`
remains supported for exactly that.

**The client is tested, not just inspected.** VS Code cannot be scripted here, but
the client can: `node editors/vscode/test/harness.js` stubs the `vscode` module,
drives the real extension against a real server, and checks the whole loop —
diagnostics appearing, clearing when the code is fixed, and hover returning the
type with its rounding contract. `cargo test` runs it when node is present and says
so loudly when it does not.

### What the server does NOT do yet

- ~~**Hover**~~ **shipped in v0.0.35**: the exact type, plus a sentence on what it
  guarantees — a `Decimal<2, RoundHalfEven>` hover names the scale *and* says
  results round half to even. It reports the SMALLEST expression under the cursor,
  and knows types only up to the first error, because the compiler stops there.
- **Go-to-definition**, which needs the compiler to keep name resolution rather
  than only its result.
- ~~**More than one error at a time**~~ **shipped in v0.0.37**: the typechecker
  recovers per statement, so every type error is published at once. Lexer, parser
  and declaration errors still arrive alone — guessing where a malformed statement
  ends would invent errors instead of finding them.
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
- [x] A brand colour taken from the artwork: `#E8502A` (see `assets/README.md`).
- [ ] Usage across enough public repositories. **This is the only open item.**
- [ ] The `languages.yml` entry, submitted as a PR:

```yaml
Burxt:
  type: programming
  color: "#E8502A"
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
