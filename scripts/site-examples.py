#!/usr/bin/env python3
"""Generate docs/examples.md by RUNNING each snippet through the compiler.

The output on the site is never typed by hand. Every panel's result comes from actually compiling
and running the program, so the page cannot claim something the compiler does not do — and
`the_site_examples_are_not_stale` in tests/runner.rs regenerates and diffs, so it cannot drift
later either.

Several panels are programs that FAIL on purpose, and their panel shows the compiler's refusal.
That is not an apology, it is the pitch: what Burxt declines to compile is the most interesting
thing about it.

    python3 scripts/site-examples.py            # writes docs/examples.md
    python3 scripts/site-examples.py --check     # exits 1 if the file on disk is out of date
"""
import json
import os
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BURXT = os.path.join(ROOT, "target", "release", "burxt")
# docs/examples/index.md, not docs/examples.md. Jekyll serves a bare `examples.md` at
# `/examples.html`, and only `<dir>/index.md` gets the directory URL — so the nav linking
# `/examples/` 404'd until the file moved. Same reason `guide/` worked from the start.
OUT = os.path.join(ROOT, "docs", "examples", "index.md")

# (id, tab label, one-line point, source). Short on purpose: a panel is a thing you read at a
# glance, not a program you study. The long ones live in examples/.
SNIPPETS = [
    ("hello", "Hello", "A whole program. No entry point to declare.",
     'print("Hello, world!");\n'),

    ("money", "Money", "Exact decimals are the default, computed as scaled integers.",
     'let price: Decimal<2> = 19.99;\n'
     'let qty:   Int        = 3;\n'
     'let total: Decimal<2> = price * qty;\n'
     'print(total);\n'),

    ("scales", "Scales", "Adding decimals of different scales is a compile error, not a rounding.",
     'let dollars: Decimal<2> = 19.99;\n'
     'let precise: Decimal<4> = 0.0825;\n'
     'print(dollars + precise);\n'),

    ("overflow", "Overflow", "Arithmetic traps rather than wrapping around quietly.",
     'let big: Int = 9223372036854775807;\n'
     'print(big + 1);\n'),

    ("absence", "No null", "Absence is a type, and both cases must be written.",
     'use "lib/option.bx";\n'
     '\n'
     'function first_even(xs: [Int]) -> Option<Int> {\n'
     '    for x in xs {\n'
     '        if remainder(x, 2) == 0 {\n'
     '            return Option.Some(x);\n'
     '        }\n'
     '    }\n'
     '    return Option.None;\n'
     '}\n'
     '\n'
     'region r {\n'
     '    let xs: [Int] = [3, 7, 8, 9];\n'
     '    match first_even(xs) {\n'
     '        None => { print("none"); }\n'
     '        Some(n) => { print(n); }\n'
     '    }\n'
     '}\n'),

    ("generics", "Generics", "One definition, one machine function per type. Nothing is erased.",
     'function largest<T: Ordered>(a: T, b: T) -> T {\n'
     '    if a > b {\n'
     '        return a;\n'
     '    }\n'
     '    return b;\n'
     '}\n'
     '\n'
     'region r {\n'
     '    print(largest(3, 9));\n'
     '    print(largest($2.50, $17.25));\n'
     '}\n'),

    ("maps", "Maps", "Iteration is insertion order, always. Never a hash order.",
     'use "lib/map.bx";\n'
     '\n'
     'region r {\n'
     '    let mutable counts: Map<String, Int> = map_new();\n'
     '    let a: Int = counts.set("pears", 7);\n'
     '    let b: Int = counts.set("apples", 3);\n'
     '    let c: Int = counts.set("plums", 1);\n'
     '    let gone: Bool = counts.remove("apples");\n'
     '\n'
     '    let names: [String] = counts.keys();\n'
     '    for name in names {\n'
     '        print(name);\n'
     '    }\n'
     '}\n'),

    ("regions", "Memory", "No collector. A region is a bump pointer and a mark.",
     'function label(n: Int) -> String allocates {\n'
     '    return "item " + to_string(n);\n'
     '}\n'
     '\n'
     'region r {\n'
     '    let mutable i: Int = 1;\n'
     '    while i <= 3 {\n'
     '        print(label(i));\n'
     '        i += 1;\n'
     '    }\n'
     '}\n'
     '// every String built above is released here, at once, in O(1)\n'),

    ("contracts", "Contracts", "A precondition is checked, and names itself when it fails.",
     'function withdraw(balance: Decimal<2>, amount: Decimal<2>) -> Decimal<2>\n'
     '    requires amount > $0.00\n'
     '    requires amount <= balance\n'
     '    ensures result >= $0.00\n'
     '{\n'
     '    return balance - amount;\n'
     '}\n'
     '\n'
     'print(withdraw($100.00, $30.00));\n'
     'print(withdraw($100.00, $500.00));\n'),
]


def run(source: str) -> tuple[str, str]:
    """Compile and run, and answer (what the user sees, which KIND of outcome it was).

    Three outcomes, not two, because "the compiler refuses this" is false for a program that
    compiles fine and then traps at run time. Overflow is the case that forced the distinction:
    `9223372036854775807 + 1` is a perfectly well-typed program, and it stops when the addition
    cannot be represented. Calling that a compile error on a website would be a lie about how the
    language works.

    From a scratch directory because `burxt run` writes its executable into the working one, and a
    stray binary in the repository is something the suite refuses. `lib/` is symlinked in so
    `use "lib/option.bx"` resolves exactly as it does for a reader who copied the snippet.
    """
    with tempfile.TemporaryDirectory() as work:
        os.symlink(os.path.join(ROOT, "lib"), os.path.join(work, "lib"))
        path = os.path.join(work, "snippet.bx")
        with open(path, "w") as f:
            f.write(source)
        done = subprocess.run(
            [BURXT, "run", "snippet.bx"], cwd=work,
            capture_output=True, text=True, timeout=60,
        )
        # Drop the compiler's own "compiled X -> Y" line: it is noise on a website.
        keep = lambda t: "\n".join(
            l for l in t.splitlines() if not l.startswith("compiled ")).strip()
        if done.returncode == 0:
            return keep(done.stdout), "ok"
        shown = keep(done.stdout + done.stderr)
        kind = "runtime" if "burxt runtime error" in shown else "compile"
        return shown, kind


def build() -> str:
    panels = []
    for ident, label, point, source in SNIPPETS:
        output, kind = run(source)
        panels.append({
            "id": ident, "label": label, "point": point,
            "source": source.rstrip("\n"), "output": output, "kind": kind,
        })

    data = json.dumps(panels, indent=1)
    tabs = "\n".join(
        '  <button role="tab" data-panel="{id}" aria-selected="{sel}">{label}</button>'.format(
            id=p["id"], label=p["label"], sel="true" if i == 0 else "false")
        for i, p in enumerate(panels)
    )

    return '''---
layout: default
title: Examples
section: examples
description: Burxt programs and exactly what the compiler does with them.
width: wide
---

# Examples

Every result below came from **running the program through the compiler**, not from typing what it
ought to say. A test regenerates this page and fails if any of it has drifted.

Some of these do not succeed, on purpose — what Burxt refuses is the most interesting thing about
it. Those panels say which kind of refusal it is, because the difference matters: a scale mismatch
is caught at **compile time**, while an overflow is a well-typed program that **stops** rather than
wrapping around quietly.

<div class="picker" role="tablist">
{tabs}
</div>

<p id="point" style="color:var(--ink-soft); margin:0 0 1rem;"></p>

<div class="pane">
  <div class="source">
    <textarea id="src" spellcheck="false" aria-label="Burxt source"></textarea>
  </div>
  <div>
    <div class="out">
      <h4 id="outhead">Output</h4>
      <pre><code id="out"></code></pre>
      <p class="note" id="note"></p>
    </div>
    <div class="cta" style="margin-top:1rem; justify-content:flex-start;">
      <a class="btn ghost" id="copy" href="#" style="font-size:14px;">Copy</a>
    </div>
  </div>
</div>

<p style="color:var(--ink-soft); font-size:14px; margin-top:2rem;">
Editing the source here will not change the output — running a compiler needs a machine, and this
page is a static file. Copy it and run it locally, or
<a href="https://codespaces.new/andrecorugda/burxt?quickstart=1">open a Codespace</a> where the real
compiler is a click away.
</p>

<script>
const PANELS = {data};

const tabs = document.querySelectorAll('.picker button');
const src = document.getElementById('src');
const out = document.getElementById('out');
const head = document.getElementById('outhead');
const note = document.getElementById('note');
const point = document.getElementById('point');
let current = PANELS[0];

function show(panel) {{
  current = panel;
  src.value = panel.source;
  out.textContent = panel.output;
  point.textContent = panel.point;
  const HEAD = {{
    ok:      ['Output', ''],
    compile: ['Refused at compile time', 'stale'],
    runtime: ['Stopped at run time', 'stale'],
  }};
  const NOTE = {{
    ok:      'Recorded by running this program.',
    compile: 'The real message from the compiler, not a paraphrase.',
    runtime: 'It typechecks. It stops when the value cannot be represented.',
  }};
  head.textContent = HEAD[panel.kind][0];
  head.className = HEAD[panel.kind][1];
  note.textContent = NOTE[panel.kind];
  const rows = panel.source.split('\\n').length;
  src.style.minHeight = Math.max(14, rows + 3) * 1.55 + 2.2 + 'rem';
  tabs.forEach(t => t.setAttribute('aria-selected', String(t.dataset.panel === panel.id)));
}}

tabs.forEach(t => t.addEventListener('click', () => {{
  show(PANELS.find(p => p.id === t.dataset.panel));
}}));

document.getElementById('copy').addEventListener('click', e => {{
  e.preventDefault();
  navigator.clipboard.writeText(src.value).then(() => {{
    const b = e.target;
    const was = b.textContent;
    b.textContent = 'Copied';
    setTimeout(() => {{ b.textContent = was; }}, 1200);
  }});
}});

show(PANELS[0]);
</script>
'''.format(tabs=tabs, data=data)


if __name__ == "__main__":
    if not os.path.exists(BURXT):
        sys.exit("build the release binary first: cargo build --release")
    page = build()
    if "--check" in sys.argv:
        on_disk = open(OUT).read() if os.path.exists(OUT) else ""
        if on_disk != page:
            sys.exit("docs/examples/index.md is out of date — run: python3 scripts/site-examples.py")
        print("docs/examples/index.md is current")
    else:
        with open(OUT, "w") as f:
            f.write(page)
        print("wrote docs/examples/index.md")
