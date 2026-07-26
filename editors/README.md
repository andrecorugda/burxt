# Editor and platform support for Burxt

A language is not real to the people using it until their editor knows it. This
directory holds that half of the project.

| Piece | State | Where |
|---|---|---|
| TextMate grammar (highlighting) | **DONE** (v0.0.31) | `vscode/syntaxes/burxt.tmLanguage.json` |
| Language configuration (comments, brackets, indent) | **DONE** (v0.0.31) | `vscode/language-configuration.json` |
| VS Code extension | **DONE** (v0.0.31) — declarative, no build step | `vscode/` |
| `burxt check` — front end only, for editors and CI | **DONE** (v0.0.31) | `src/main.rs` |
| Diagnostics with line/column | **NEXT** — needs source spans in the compiler | see below |
| Language server (`burxt lsp`) | after spans | see below |
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

**A tree-sitter grammar.** Neovim, Helix, and GitHub's own newer highlighting
path all want one. It is a real gap; writing two grammars that can disagree is
also a real cost, so it waits until something needs it.

**Formatting (`burxt fmt`).** A formatter is a design decision about the
language's canonical shape, not a tooling detail — it deserves its own spec
rather than being improvised.

## Diagnostics and the language server

The plan, in dependency order, because the pieces genuinely block each other:

1. **Source spans in the compiler.** Every error today is a bare string:
   `error: this function returns Int, but ...`. Useful in a terminal, useless to
   an editor, which needs a line, a column, and a length to underline. This is
   the blocker for everything below, and it improves the CLI at the same time —
   real compilers print the offending line with a caret.
2. **Machine-readable diagnostics** from `burxt check`, so any editor with a
   problem matcher gets squiggles without an LSP at all.
3. **`burxt lsp`** — a language server over stdio: diagnostics on change,
   hover showing a value's exact type (a `Decimal<2, RoundHalfEven>` hover is
   worth more here than in most languages), and go-to-definition.

The order matters: an LSP that cannot say *where* a problem is would be a shell
with nothing inside it.

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
