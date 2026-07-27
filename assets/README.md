# Burxt "b" mark — white X, bold, angular ends

Lowercase b in red-orange (#E8502A) with a BOLD WHITE X inside the bowl.
The X has flat/angled ends (matches the wordmark X) and is filled white, so it
contrasts on any background — light, dark, or transparent. b + x = Burxt.

## Icon
- burxt-b-icon.svg / .png
- burxt-b-favicon-16/32/48/180/512.png
- favicon.ico

## Lockups (b in rounded rectangle + "burxt" at tile height)
- burxt-lockup-light.png / burxt-lockup-dark.png / burxt-lockup-outline.png

## Web
<link rel="icon" href="/favicon.ico" sizes="any">
<link rel="icon" type="image/png" href="/burxt-b-favicon-32.png" sizes="32x32">
<link rel="apple-touch-icon" href="/burxt-b-favicon-180.png">

---

## Where this repository uses them

Added below the original notes rather than edited into them, so the brief above
stays as written.

| File | Used by |
|---|---|
| `burxt-b-favicon-512.png` | `editors/vscode/icon.png` — the VS Code extension tile. |
| `burxt-b-favicon-48.png` | `editors/vscode/file-icon.png` — the icon for `.bx` files in the explorer. One file serves both themes: the mark is red-orange with a white X, which reads on light and dark alike. |
| `burxt-lockup-light.png`, `burxt-lockup-dark.png` | The README banner, switched by `prefers-color-scheme`. |

The extension keeps its own copies because VS Code resolves contributed paths
relative to the extension directory, not the repository root. Re-copy them if the
artwork changes, and repackage:

```sh
cp assets/burxt-b-favicon-512.png editors/vscode/icon.png
cp assets/burxt-b-favicon-48.png  editors/vscode/file-icon.png
python3 editors/vscode/pack.py
```

## The palette, sampled from the artwork

| Colour | Hex | Where |
|---|---|---|
| Ember | `#E8502A` | The `b`, and the wordmark |
| White | `#FFFFFF` | The X inside the bowl |
| Obsidian | `#232320` | The dark lockup's ground |

Read out of `burxt-b-favicon-512.png` and `burxt-lockup-dark.png` pixel by pixel
rather than eyeballed, so anything that needs the brand colour in text — the GitHub
Linguist entry, a site stylesheet — uses the value the artwork actually contains.

The mark is also the only artwork here that exists as **vector**: `burxt-b-icon.svg`
is 445 bytes — a bar, a bowl, and two crossing polygons — so any size can be
regenerated from it without a raster editor.
