#!/usr/bin/env python3
"""Generate examples/refused/README.md by RUNNING every program through the compiler.

Every message on that page is the compiler's, captured. Nothing is typed by hand, and
`the_refusals_page_is_not_stale` in tests/runner.rs regenerates and diffs — so the page cannot
claim a refusal the compiler does not make, and cannot drift when a message is improved.

That matters more here than anywhere else in the repository. This page IS the argument: it exists
to show a reviewer a mistake they would not have caught, and a made-up error message would make it
a lie about the one thing it is selling. The guide already told that lie once, with two invented
messages, and running the examples is what caught it.

    python3 scripts/refused.py             # writes examples/refused/README.md
    python3 scripts/refused.py --check     # exits 1 if the file on disk is out of date
"""
import os
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
# The compiler to run. `BURXT` in the environment wins, so a caller that already has a binary — the
# test suite, which is handed one by cargo whatever profile it built — can say which. Falling back to
# the release build is right for a human typing this by hand.
#
# It used to be the release path and nothing else, and CI paid for thirteen versions: CI builds debug,
# so the release binary was never there. One test hard-failed on it and another SKIPPED silently,
# which is the worse half — a check that has never run in CI looks exactly like one that passes.
BURXT = os.environ.get("BURXT") or os.path.join(ROOT, "target", "release", "burxt")
DIR = os.path.join(ROOT, "examples", "refused")
OUT = os.path.join(DIR, "README.md")
# The same panels as a website page. Generated from the same run, so the site cannot say something
# the repository does not — which was the whole reason for generating either of them.
SITE = os.path.join(ROOT, "docs", "refused", "index.md")

# What each one is, in a reviewer's terms rather than a compiler's.
POINTS = {
    "01-mixed-scales": ("A rate added to a price", "Both are decimals. One is money and one is a multiplier."),
    "02-unrounded-product": ("Money times a rate, unrounded", "The exact answer has six decimal places. Something has to decide which way two of them go."),
    "03-silent-overflow": ("A total past what an Int holds", "Every other language wraps this to a negative and keeps going."),
    "04-bypassed-private": ("The constructor skipped", "The class checks its invariant on the way in, so build one directly instead."),
    "05-forgotten-variant": ("A case added after the code was written", "A payment method joined the enum. This `match` predates it."),
    "06-violated-contract": ("A precondition passed a value it forbids", "The contract is in the signature, and the call still breaks it."),
    "07-escaping-region": ("Memory returned after it was freed", "Built inside a `region` for tidiness, then handed to the caller."),
    "08-text-as-money": ("Text treated as money", "A number that arrived from a model, a form or a CSV."),
    "09-incomplete-interface": ("An interface gained a method", "The class satisfied it last week and still looks complete."),
    "10-comparing-kinds": ("A count compared with a price", "Both are numbers to a human and to every dynamic language."),
}


def run(path: str) -> tuple[str, str]:
    """Compile and run, and answer (what the reviewer sees, which KIND of refusal).

    Three outcomes, not two, and the distinction is the honest part: a scale mismatch is caught
    at COMPILE time, while an overflow is a well-typed program that STOPS when the value cannot be
    represented. Calling the second a compile error would be a lie about how the language works.

    From a scratch directory, because `burxt run` writes its executable into the working one and a
    stray binary in the repository is something the suite refuses.
    """
    with tempfile.TemporaryDirectory() as work:
        done = subprocess.run(
            [BURXT, "run", path, "-o", os.path.join(work, "out")],
            capture_output=True, text=True, timeout=60, cwd=work,
        )
    keep = lambda t: "\n".join(
        l for l in t.splitlines() if not l.startswith("compiled ")).strip()
    if done.returncode == 0:
        return keep(done.stdout), "accepted"
    shown = keep(done.stdout + done.stderr)
    kind = "runtime" if "burxt runtime error" in shown else "compile"
    return shown, kind


def build() -> str:
    names = sorted(
        f[:-3] for f in os.listdir(DIR) if f.endswith(".bx")
    )
    panels = []
    for name in names:
        source = open(os.path.join(DIR, name + ".bx")).read().rstrip("\n")
        message, kind = run(os.path.join(DIR, name + ".bx"))
        panels.append((name, source, message, kind))

    caught = sum(1 for _, _, _, k in panels if k != "accepted")
    parts = [f"""# What Burxt refuses

{caught} mistakes that **compile in every other language**, and what this compiler says instead.

Each one is code an agent writes confidently: it type-checks in Python, runs in PHP, and passes
review because nothing about it looks wrong. Read them and ask, honestly, which you would have
caught in a pull request at 5pm.

That is the whole argument. `examples/pos/` shows that the money is exact — this shows the part
that matters more: **every one of these is a review you no longer have to do.**

Two kinds of refusal appear below and the difference is not cosmetic. Most are caught at
**compile time**, before the program exists. One is a well-typed program that **stops** when a
value cannot be represented — calling that a compile error would misdescribe how the language
works.

Every message here was produced by running the program. `scripts/refused.py` regenerates this
file and a test diffs it, so the page cannot claim a refusal the compiler does not make.

---
"""]
    for name, source, message, kind in panels:
        title, why = POINTS.get(name, (name, ""))
        label = {
            "compile": "Refused at compile time",
            "runtime": "Stopped at run time",
            "accepted": "**ACCEPTED — this panel is stale**",
        }[kind]
        parts.append(f"""## {title}

{why}

```burxt
{source}
```

**{label}:**

```
{message}
```
""")
    parts.append("""---

## What is not here

Nothing about performance, and nothing about syntax. Every refusal above is a **wrong answer
prevented** — a total that would have been short by a cent, a balance that would have gone
negative, a case that would have fallen through, freed memory handed back to a caller.

The list is also incomplete on purpose. `tests/fail/` holds over two hundred more, each with the
exact message it must produce, because a refusal that is not tested is a refusal that will
eventually stop happening.
""")
    return "\n".join(parts)


def site_page(body: str) -> str:
    """The same page with Jekyll front matter, so it renders on burxt-lang.org."""
    front = """---
layout: default
title: What it refuses
section: refused
description: Ten mistakes that compile in every other language, and what this compiler says instead.
---

"""
    # The repository page links to sibling directories; the site version links to the site.
    return front + body.replace("(../../tests/fail/", "(https://github.com/andrecorugda/burxt/tree/main/tests/fail/")


if __name__ == "__main__":
    if not os.path.exists(BURXT):
        sys.exit("build the release binary first: cargo build --release")
    page = build()
    site = site_page(page)
    if "--check" in sys.argv:
        for path, wanted in ((OUT, page), (SITE, site)):
            on_disk = open(path).read() if os.path.exists(path) else ""
            if on_disk != wanted:
                sys.exit(f"{path} is out of date — run: python3 scripts/refused.py")
        print("the refusals page is current, in both places")
    else:
        os.makedirs(os.path.dirname(SITE), exist_ok=True)
        for path, text in ((OUT, page), (SITE, site)):
            with open(path, "w") as f:
                f.write(text)
        print("wrote examples/refused/README.md and docs/refused/index.md")
