---
layout: default
title: Packages
section: packages
description: "Every Burxt package that exists, with the one line each needs to be used."
---


# Packages

A Burxt package is **a git repository with a tag**. There is no registry to publish to and no account to create: the URL is the package's name, so nothing central has to agree that it exists. This page is the index — somewhere to look, which is the only part a list cannot replace.

To use one, name it in your `burxt.package` and fetch once. A build never touches the network, so the fetch is a separate step you run deliberately:

```sh
burxt fetch
```

## bmx

The BMX markup format — a parser and conformance suite, with the Burxt implementation under `burxt/`.

```
dependency  bmx  https://github.com/andrecorugda/bmx  burxt-0.12.2
```

Then `use "bmx/<file>.bx";` from anywhere in your program. [The repository](https://github.com/andrecorugda/bmx).

> Declares no `burxt.package` of its own. It resolves anyway — a package import is a path join under the fetched directory — so this is a note rather than a warning.

## star-burxt

Components for Burxt: a `.sbmx` document is a component, and the generator turns one into Burxt source the compiler then judges.

```
dependency  star-burxt  https://github.com/andrecorugda/star-burxt  v0.2.0
```

Then `use "star-burxt/<file>.bx";` from anywhere in your program. [The repository](https://github.com/andrecorugda/star-burxt).

## What you get for free, without any of this

The standard library is **not** a package. `lib/` ships beside the compiler and `use "std/string.bx";` reaches it with nothing declared and nothing fetched — which is why it resolves against exactly two roots, `BURXT_LIB` and the `lib/burxt` next to the running binary, and no others. A dependency you did not ask for is a dependency you cannot review.

## Publishing one

Tag a repository. That is the whole procedure — there is nothing to upload and nobody to ask. Two things make yours pleasant to depend on:

- **Declare a `burxt.package`** with a `name` and a `version`, so your package states its own identity rather than being known only by the URL somebody typed.
- **Mark your interface `public`.** Everything is visible inside a package; only `public` declarations are reachable from a package that depends on yours, so what you do not mark stays yours to change. `burxt review --semver` then answers mechanically whether your next tag is a patch, a minor or a major.

To be found, add the **`burxt-lang`** topic to your repository. [github.com/topics/burxt-lang](https://github.com/topics/burxt-lang) is then an index GitHub maintains for you, and it costs one command:

```sh
gh repo edit --add-topic burxt-lang
```

To be listed here as well, open a pull request adding an entry to `scripts/site-packages.bx`. The list is authored rather than scraped, because a page a reader trusts should not contain whatever a search happened to return.

