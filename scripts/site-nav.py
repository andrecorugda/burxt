#!/usr/bin/env python3
"""Generate docs/_data/nav.yml — the sidebar, read out of the pages it navigates.

    python3 scripts/site-nav.py            # writes docs/_data/nav.yml
    python3 scripts/site-nav.py --check    # exits 1 if the file on disk is out of date

The site had no sidebar at all: twelve guide pages, each a document you could only reach by going
back to an index, with no way to see what a page contained before opening it and no way to see what
the language contained at all. The report was "there is no side bar where I can explore topics and
sub topics".

A sidebar needs the headings of every page while rendering any one of them, and Jekyll on GitHub
Pages allows no plugins — so it is built here and read from Liquid as `site.data.nav`. Extracting
H2s in Liquid is possible and would be a page-scanning loop per request; a generated data file with
a test that diffs it is the same discipline `site-examples.py` and `refused.py` already use.

The heading anchors have to match what kramdown generates, because they are what the links point at.
kramdown lowercases, drops anything that is not a letter, digit, space or hyphen, and joins on
hyphens — `generate_id` in its HTML converter. `slug` below is that, and
`the_sidebar_anchors_match_the_headings` in tests/runner.rs holds it to a page it can verify.
"""
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "docs", "_data", "nav.yml")

# The reference's pages, in the order the reference itself lists them. Kept beside
# scripts/site-reference.py's MODULES rather than derived from the directory, because alphabetical
# would put `builtins` between `os` and `result` and the reading order is the point.
REFERENCE = [
    ("index", "Overview"),
    ("builtins", "Builtins"),
    ("cli", "The command line"),
    ("option", "lib/option.bx"),
    ("result", "lib/result.bx"),
    ("string", "lib/string.bx"),
    ("map", "lib/map.bx"),
    ("json", "lib/json.bx"),
    ("files", "lib/files.bx"),
    ("os", "lib/os.bx"),
]


def label(text):
    """A heading as the sidebar should SHOW it — the words, without the markdown.

    Without this the sidebar reads ``print`` with the backticks in it, because a heading's source is
    markdown and Liquid prints the string it is given.
    """
    text = re.sub(r"\[([^\]]*)\]\([^)]*\)", r"\1", text)
    text = text.replace("`", "")
    text = re.sub(r"\*\*(.+?)\*\*", r"\1", text)
    text = re.sub(r"\*(.+?)\*", r"\1", text)
    return re.sub(r"\s+", " ", text).strip()


def slug(text):
    """kramdown's `basic_generate_id`, character for character.

    This is a FALLBACK. A page that states its own id with `{: #…}` is believed instead, and the
    generated reference pages all do — because reproducing another project's slug rule is exactly
    the kind of near-miss that fails silently: the link resolves, the page loads, and the reader
    lands at the top instead of at the section.

    The rule, from kramdown's HTML converter, and each line of it matters:

        gen_id = str.gsub(/^[^a-zA-Z]+/, '')   # a leading "1. " is dropped entirely
        gen_id.tr!('^a-zA-Z0-9 -', '')         # underscores are DELETED, not hyphenated
        gen_id.tr!(' ', '-')                   # one hyphen per space, so two spaces give two
        gen_id.downcase!

    So `to_string` is `tostring`, not `to-string`, and `region — a tray` keeps a double hyphen where
    the em dash was removed from between two spaces. Neither is what you would guess.
    """
    # kramdown hashes the heading's TEXT, after inline markup has been parsed away.
    text = re.sub(r"\[([^\]]*)\]\([^)]*\)", r"\1", text)
    text = text.replace("`", "")
    text = re.sub(r"\*\*(.+?)\*\*", r"\1", text)
    text = re.sub(r"\*(.+?)\*", r"\1", text)

    text = re.sub(r"^[^a-zA-Z]+", "", text)
    text = re.sub(r"[^a-zA-Z0-9 \-]", "", text)
    text = text.replace(" ", "-")
    return text.lower() or "section"


# `{: #an-id}` on the line after a heading — kramdown's inline attribute list. A page that says what
# its anchor is does not need anyone to work it out.
IAL = re.compile(r"^\{:\s*#([A-Za-z0-9_-]+)\s*\}\s*$")


def headings(path):
    """Every H2 on a page, with the id it will actually carry. Fences are skipped."""
    found = []
    fenced = False
    pending = None
    with open(path) as f:
        for line in f:
            line = line.rstrip("\n")
            if line.startswith("```"):
                fenced = not fenced
                continue
            if fenced:
                continue
            if pending is not None:
                m = IAL.match(line.strip())
                found.append((pending, m.group(1) if m else slug(pending)))
                pending = None
                if m:
                    continue
            if line.startswith("## "):
                pending = line[3:].strip()
    if pending is not None:
        found.append((pending, slug(pending)))
    return found


def title_of(path):
    """The H1, which is what the page calls itself."""
    with open(path) as f:
        for line in f:
            if line.startswith("# "):
                return line[2:].strip()
    return os.path.basename(path)


def quote(s):
    return '"%s"' % s.replace("\\", "\\\\").replace('"', '\\"')


def build():
    out = [
        "# The sidebar. GENERATED by scripts/site-nav.py — do not edit.",
        "#",
        "# Every entry is read out of the page it links to, so a renamed heading moves its own",
        "# sidebar entry and cannot leave a dead anchor behind. A test regenerates this and diffs.",
        "",
    ]

    guide = os.path.join(ROOT, "docs", "guide")
    pages = sorted(p for p in os.listdir(guide) if re.match(r"^\d\d-.*\.md$", p))
    if len(pages) < 11:
        sys.exit("found only %d numbered guide pages — that cannot be right" % len(pages))

    out.append("- title: The guide")
    out.append("  url: /guide/")
    out.append("  items:")
    for page in pages:
        path = os.path.join(guide, page)
        out.append("    - name: %s" % quote(label(title_of(path))))
        out.append("      url: /guide/%s.html" % page[:-3])
        steps = headings(path)
        if steps:
            out.append("      steps:")
            for text, ident in steps:
                out.append("        - name: %s" % quote(label(text)))
                out.append("          id: %s" % ident)

    reference = os.path.join(ROOT, "docs", "reference")
    if os.path.isdir(reference):
        out.append("- title: Reference")
        out.append("  url: /reference/")
        out.append("  items:")
        for name, shown in REFERENCE:
            path = os.path.join(reference, name + ".md")
            if not os.path.exists(path):
                sys.exit(
                    "docs/reference/%s.md is missing. Run: python3 scripts/site-reference.py"
                    % name
                )
            url = "/reference/" if name == "index" else "/reference/%s.html" % name
            out.append("    - name: %s" % quote(shown))
            out.append("      url: %s" % url)
            steps = headings(path)
            if steps:
                out.append("      steps:")
                for text, ident in steps:
                    out.append("        - name: %s" % quote(label(text)))
                    out.append("          id: %s" % ident)

    return "\n".join(out) + "\n"


def main():
    text = build()
    if "--check" in sys.argv:
        on_disk = open(OUT).read() if os.path.exists(OUT) else ""
        if on_disk != text:
            sys.exit("docs/_data/nav.yml is out of date — run: python3 scripts/site-nav.py")
        print("docs/_data/nav.yml is current")
        return
    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    with open(OUT, "w") as f:
        f.write(text)
    print("wrote docs/_data/nav.yml")


if __name__ == "__main__":
    main()
