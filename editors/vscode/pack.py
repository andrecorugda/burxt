#!/usr/bin/env python3
"""Package the extension as a .vsix, with no toolchain.

    python3 editors/vscode/pack.py            # writes burxt-<version>.vsix here
    code --install-extension editors/vscode/burxt-0.1.0.vsix

A .vsix is a ZIP with three things in it: an OPC content-types map, a VSIX
manifest, and the extension under `extension/`. `vsce` does more than this —
linting, dependency bundling, marketplace checks — but all of that is for
publishing, and none of it is needed to install locally. Since the extension has
no npm dependencies, a packer in the standard library is the whole job, and it
keeps the promise that this directory needs no toolchain to use.

Why package at all rather than symlinking the folder into the extensions
directory: an installed extension is registered, versioned, upgradable and
uninstallable through the normal UI, and it is the same shape everyone else's
extensions have. A symlink works, until something scans the registry and does not
find you.
"""

import json
import zipfile
from pathlib import Path

HERE = Path(__file__).resolve().parent

# Everything that belongs in the package. Listed rather than globbed, so a stray
# file in the directory never ships by accident.
FILES = [
    "package.json",
    "extension.js",
    "language-configuration.json",
    "syntaxes/burxt.tmLanguage.json",
    "icon.png",
    "file-icon.png",
    "README.md",
]

CONTENT_TYPES = """<?xml version="1.0" encoding="utf-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension=".json" ContentType="application/json"/>
  <Default Extension=".js" ContentType="application/javascript"/>
  <Default Extension=".png" ContentType="image/png"/>
  <Default Extension=".svg" ContentType="image/svg+xml"/>
  <Default Extension=".md" ContentType="text/markdown"/>
  <Default Extension=".xml" ContentType="text/xml"/>
  <Default Extension=".vsixmanifest" ContentType="text/xml"/>
</Types>
"""


def escape(text):
    return (
        text.replace("&", "&amp;")
        .replace("<", "&lt;")
        .replace(">", "&gt;")
        .replace('"', "&quot;")
    )


def manifest(pkg):
    tags = ",".join(pkg.get("keywords", []))
    categories = ",".join(pkg.get("contributes", {}) and pkg.get("categories", []))
    # `workspace` matters on a remote (WSL, SSH, container): the extension spawns
    # the compiler and the language server, so it has to run where the code is,
    # not on the UI side.
    kind = ",".join(pkg.get("extensionKind", ["workspace"]))
    return f"""<?xml version="1.0" encoding="utf-8"?>
<PackageManifest Version="2.0.0" xmlns="http://schemas.microsoft.com/developer/vsx-schema/2011" xmlns:d="http://schemas.microsoft.com/developer/vsx-schema-design/2011">
  <Metadata>
    <Identity Language="en-US" Id="{escape(pkg['name'])}" Version="{escape(pkg['version'])}" Publisher="{escape(pkg['publisher'])}"/>
    <DisplayName>{escape(pkg.get('displayName', pkg['name']))}</DisplayName>
    <Description xml:space="preserve">{escape(pkg.get('description', ''))}</Description>
    <Tags>{escape(tags)}</Tags>
    <Categories>{escape(categories)}</Categories>
    <GalleryFlags>Public</GalleryFlags>
    <Properties>
      <Property Id="Microsoft.VisualStudio.Code.Engine" Value="{escape(pkg['engines']['vscode'])}"/>
      <Property Id="Microsoft.VisualStudio.Code.ExtensionDependencies" Value=""/>
      <Property Id="Microsoft.VisualStudio.Code.ExtensionPack" Value=""/>
      <Property Id="Microsoft.VisualStudio.Code.ExtensionKind" Value="{escape(kind)}"/>
      <Property Id="Microsoft.VisualStudio.Services.Links.Source" Value="{escape(pkg.get('repository', {}).get('url', ''))}"/>
    </Properties>
    <Icon>extension/icon.png</Icon>
  </Metadata>
  <Installation>
    <InstallationTarget Id="Microsoft.VisualStudio.Code"/>
  </Installation>
  <Dependencies/>
  <Assets>
    <Asset Type="Microsoft.VisualStudio.Code.Manifest" Path="extension/package.json" Addressable="true"/>
    <Asset Type="Microsoft.VisualStudio.Services.Content.Details" Path="extension/README.md" Addressable="true"/>
    <Asset Type="Microsoft.VisualStudio.Services.Icons.Default" Path="extension/icon.png" Addressable="true"/>
  </Assets>
</PackageManifest>
"""


def main():
    pkg = json.loads((HERE / "package.json").read_text())
    out = HERE / f"{pkg['name']}-{pkg['version']}.vsix"

    missing = [f for f in FILES if not (HERE / f).exists()]
    if missing:
        raise SystemExit(f"cannot package, these are missing: {missing}")

    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("[Content_Types].xml", CONTENT_TYPES)
        z.writestr("extension.vsixmanifest", manifest(pkg))
        for name in FILES:
            z.write(HERE / name, f"extension/{name}")

    print(f"wrote {out.relative_to(HERE.parent.parent)} ({out.stat().st_size} bytes)")
    print("install with:  code --install-extension", out)


if __name__ == "__main__":
    main()
