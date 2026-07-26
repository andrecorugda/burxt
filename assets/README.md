# Burxt icon — your artwork, background removed

These are your original X artwork with the white background removed — not traced,
not redrawn. Your exact pixels, now transparent.

## Files
- burxt-icon.png            — full-res transparent icon (928x792), cropped tight.
- burxt-favicon-16/32/48.png — favicon sizes, transparent, square.
- burxt-favicon-180.png     — apple-touch-icon size, transparent.
- burxt-favicon-512.png     — large / PWA icon, transparent.
- burxt-favicon-tile-180/512.png — X on obsidian (#232320) rounded tile.
- favicon.ico              — multi-size .ico (16/32/48) for the browser tab.

## Web usage
<link rel="icon" href="/favicon.ico" sizes="any">
<link rel="icon" type="image/png" href="/burxt-favicon-32.png" sizes="32x32">
<link rel="apple-touch-icon" href="/burxt-favicon-180.png">

## Lockups (icon + wordmark)
- burxt-lockup.png          — icon + Burxt wordmark, transparent background.
- burxt-lockup-light.png    — same, on white.
- burxt-lockup-dark.png     — same, on obsidian (#232320).
- *-600.png                 — 600px-wide web versions.
Use the lockup as the primary logo (site header, README banner). Use the standalone
icon/favicon where space is tight (browser tab, app icon, social avatar).

---

## Where this repository uses them

Added below the original notes rather than edited into them, so the brief above
stays as written.

| File | Used by |
|---|---|
| `burxt-favicon-tile-512.png` | `editors/vscode/icon.png` — the VS Code extension tile. A filled tile rather than the transparent icon, because the extensions list shows it on its own background. |
| `burxt-favicon-48.png` | `editors/vscode/file-icon.png` — the icon for `.bx` files in the explorer. Transparent, and copper reads on both light and dark themes, so one file serves both. |
| `burxt-lockup-light-600.png`, `burxt-lockup-dark-600.png` | The README banner, switched by `prefers-color-scheme`. |

The extension keeps its own copies because VS Code resolves contributed paths
relative to the extension directory, not the repository root. Re-copy them if the
artwork changes:

```sh
cp assets/burxt-favicon-tile-512.png editors/vscode/icon.png
cp assets/burxt-favicon-48.png       editors/vscode/file-icon.png
```

## The palette, sampled from the artwork

| Colour | Hex | Where |
|---|---|---|
| Copper | `#b26436` | The mark, and the wordmark |
| Obsidian | `#232320` | The tile, and the dark lockup's ground |

These were read out of `burxt-favicon-512.png` pixel by pixel rather than eyeballed,
so anything that needs the brand colour in text — the GitHub Linguist entry, a site
stylesheet — uses the same value the artwork actually contains.
