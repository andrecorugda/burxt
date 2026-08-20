#!/usr/bin/env python3
"""Package the extension as a .vsix, with no toolchain.

    python3 editors/vscode/pack.py            # writes burxt.vsix here
    code --install-extension editors/vscode/burxt.vsix

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
    # `burxt.package` and `burxt.lock` are matched by filename and get their own grammars. Listed
    # here as well as in `package.json`, and the test that reads the manifest's contributions back
    # out of the archive is what keeps the two lists from disagreeing.
    "manifest-language-configuration.json",
    "syntaxes/burxt.tmLanguage.json",
    "syntaxes/burxt-package.tmLanguage.json",
    "syntaxes/burxt-lock.tmLanguage.json",
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
    #
    # **Raised rather than defaulted**, which star-burxt's copy of this packer got right first: a
    # default is invisible when it is wrong, and the wrong value here loads the extension on the UI
    # side of a remote, where the compiler it spawns does not exist. The manifest must not be able to
    # say `workspace` because a packer assumed it.
    if "extensionKind" not in pkg:
        raise SystemExit(
            "package.json declares no extensionKind. This extension spawns the compiler, so it "
            'must say ["workspace"] — a packer guessing it is how a remote loads it UI-side.'
        )
    kind = ",".join(pkg["extensionKind"])
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
    # **The filename carries no version, and that is a fix rather than laziness.** It used to carry
    # the version, which put the number in five places outside `package.json` — this
    # docstring, `README.md`, `editors/README.md`, the getting-started guide, and a glob in
    # `.devcontainer/setup.sh`. The predicted failure had already happened and nobody had noticed:
    # **at version 0.1.4, `README.md` and `editors/README.md` both said 0.1.3**, so the install
    # command in the front door named a file `pack.py` does not write. BMX measured the same shape
    # from the other end — thirty commits to the package with the version never moving off 0.1.0.
    #
    # A version belongs where a tool reads it: `package.json`, and the manifest built from it here.
    # VS Code compares THAT to decide whether to offer an upgrade, and it never read the filename.
    # A stable name means the documented command is correct forever and a bump costs one edit.
    # `the_documented_install_command_names_the_file_pack_py_writes` fails if this drifts again.
    out = HERE / f"{pkg['name']}.vsix"

    missing = [f for f in FILES if not (HERE / f).exists()]
    if missing:
        raise SystemExit(f"cannot package, these are missing: {missing}")

    # **The manifest is built BEFORE the archive is opened**, because it is the step that can refuse.
    # Built inside the `with`, a refusal left a 353-byte .vsix on disk holding nothing but the
    # content-types map — and a truncated package is worse than no package: `code
    # --install-extension` is what discovers it, one machine away from the person who could fix it.
    # Nothing that can say no belongs downstream of a file being created.
    manifest_xml = manifest(pkg)

    # **One fixed stamp on every entry, so packing twice gives identical bytes.** This was NOT here,
    # and `lib/zip.bx`'s reader found it: the shipped `.vsix` carried **eight distinct timestamps**,
    # one per file mtime, so the committed artefact was never reproducible and nothing said so.
    #
    # `z.write(path, arcname)` takes the stamp from the file, and `z.writestr(str, ...)` takes it from
    # the clock — so two of the three shapes here moved on every run and the third moved whenever a
    # source file was touched. A committed archive that cannot be reproduced cannot be checked
    # against its source.
    #
    # **And the check that would have hidden it is packing twice and comparing.** A ZIP stores
    # timestamps at two-second granularity, so back-to-back packs land in one bucket and agree. The
    # property has to be asserted of the archive — every entry carrying the SAME stamp — which is
    # what star-burxt's checker does and what this now satisfies.
    def entry(name):
        info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
        info.compress_type = zipfile.ZIP_DEFLATED
        info.external_attr = 0o644 << 16
        return info

    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr(entry("[Content_Types].xml"), CONTENT_TYPES)
        z.writestr(entry("extension.vsixmanifest"), manifest_xml)
        for name in FILES:
            z.writestr(entry(f"extension/{name}"), (HERE / name).read_bytes())

    print(f"wrote {out.relative_to(HERE.parent.parent)} ({out.stat().st_size} bytes)")
    print("install with:  code --install-extension", out)


if __name__ == "__main__":
    main()
