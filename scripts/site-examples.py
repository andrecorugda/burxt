#!/usr/bin/env python3
"""Generate docs/examples/index.md by RUNNING each program through the compiler.

    python3 scripts/site-examples.py            # writes docs/examples/index.md
    python3 scripts/site-examples.py --check    # exits 1 if the file on disk is out of date

Nothing on that page is typed by hand. Every result comes from actually compiling and running the
program, and `the_site_is_honest_and_complete` in tests/runner.rs regenerates this and diffs, so it
cannot drift later either.

## What changed, and why

This page used to be nine snippets of two to eight lines in an editable `<textarea>` that could not
run anything — and it said so, in a paragraph apologising for itself. Asked whether those were really
examples, the honest answer was no: they were illustrations of single rules, which is what the guide
is for.

So it now shows **complete programs**, and the first of them is the one argument the repository had
lying around unused: `examples/pos/` is a point-of-sale till, and `examples/pos-php/`,
`examples/pos-python/` and `examples/pos-rust/` are the same program written three other ways. Reading
them beside each other is more persuasive than any paragraph about exact decimals, because the
difference is visible — in the other three the rounding rule is an argument somebody has to remember
to pass, and in Burxt it is in the return type where a reviewer cannot miss it.

The short snippets moved into the guide's `## Examples` sections, where a reader meets them in
context.

## What is NOT on the page

Whether the ports agree with the Burxt program. They do, but saying so here would mean running `php`,
`python3` and `rustc` while generating — and then this page's content would depend on which runtimes
happened to be installed, so CI and a laptop would produce different files and `--check` would fail
for a reason that has nothing to do with the site. `the_ports_agree_with_the_original` in
tests/runner.rs makes that claim instead, and skips a runtime it cannot find.
"""
import json
import os
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
# `BURXT` in the environment wins, so the test suite can say which binary — it is handed one by cargo
# whatever profile it built. Falling back to release is right for a human typing this by hand.
#
# It used to be the release path and nothing else, and CI paid for thirteen versions: CI builds debug,
# so the release binary was never there. One test hard-failed on it and another SKIPPED silently,
# which is the worse half — a check that has never run looks exactly like one that passes.
BURXT = os.environ.get("BURXT") or os.path.join(ROOT, "target", "release", "burxt")
# docs/examples/index.md, not docs/examples.md. Jekyll serves a bare `examples.md` at
# `/examples.html`, and only `<dir>/index.md` gets the directory URL — so the nav linking
# `/examples/` 404'd until the file moved.
OUT = os.path.join(ROOT, "docs", "examples", "index.md")
GH = "https://github.com/andrecorugda/burxt/blob/main"

# The requests the MCP panel shows. Two succeed, two are refused, and the refusals are the point: a
# violated precondition answers a JSON-RPC error rather than taking the process down, and a third
# decimal place asked to fit in a `Decimal<2>` is refused rather than rounded.
MCP_REQUESTS = [
    '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":'
    '{"name":"line_total","arguments":{"unit":"19.99","quantity":3}}}',
    '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":'
    '{"name":"tax_on","arguments":{"subtotal":"59.97","rate":"0.0825"}}}',
    '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":'
    '{"name":"line_total","arguments":{"unit":"0.00","quantity":3}}}',
    '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":'
    '{"name":"line_total","arguments":{"unit":"19.999","quantity":1}}}',
]

PROGRAMS = [
    {
        "id": "till",
        "label": "A till",
        "title": "A point-of-sale till, four ways",
        "point": "The same program in Burxt, PHP, Python and Rust. Only one of them puts the "
                 "rounding rule where a reviewer cannot miss it.",
        "files": ["examples/pos/items.bx", "examples/pos/tax.bx",
                  "examples/pos/receipt.bx", "examples/pos/till.bx"],
        "run": "examples/pos/till.bx",
        "ports": {
            "PHP": ["examples/pos-php/items.php", "examples/pos-php/tax.php",
                    "examples/pos-php/receipt.php", "examples/pos-php/till.php"],
            "Python": ["examples/pos-python/items.py", "examples/pos-python/tax.py",
                       "examples/pos-python/receipt.py", "examples/pos-python/till.py"],
            "Rust": ["examples/pos-rust/items.rs", "examples/pos-rust/tax.rs",
                     "examples/pos-rust/receipt.rs", "examples/pos-rust/till.rs"],
        },
        "read": "Look at `tax` in each. In Burxt the rounding is `Decimal<2, RoundHalfEven>` in the "
                "**return type**, so every caller sees it. In the other three it is a rounding mode "
                "passed as an argument, or a default nobody wrote down — and a reviewer reading the "
                "call site cannot tell which.",
    },
    {
        "id": "mcp",
        "label": "An MCP server",
        "title": "An MCP server whose schema cannot drift",
        "point": "The tool schema is derived from the preconditions, so there is no second artifact "
                 "to keep in step.",
        "files": ["examples/mcp/tools.bx", "examples/mcp/server.bx"],
        "schema": "examples/mcp/tools.bx",
        "requests": MCP_REQUESTS,
        "read": "`line_total` carries `[> $0.00]` and `[> 0, <= 100000]`. Those brackets ARE the JSON "
                "Schema — `burxt mcp-schema` reads the declaration and emits it. Nothing else in this "
                "repository, and nothing in any other language, can do that: it needs the contract to "
                "be in the signature.",
    },
    {
        "id": "invoice",
        "label": "An invoice",
        "title": "An invoice, priced and taxed",
        "point": "Money in, money out, and every rounding named at the line where it happens.",
        "files": ["examples/invoice.bx"],
        "run": "examples/invoice.bx",
    },
    {
        "id": "contracts",
        "label": "Contracts",
        "title": "A program that states what it requires",
        "point": "A precondition is checked, and it names itself when it fails.",
        "files": ["examples/contracts.bx"],
        "run": "examples/contracts.bx",
    },
    {
        "id": "generics",
        "label": "Generics",
        "title": "One definition, one machine function per type",
        "point": "Nothing is erased. A `Stack<Int>` and a `Stack<String>` are two real types.",
        "files": ["examples/generics.bx"],
        "run": "examples/generics.bx",
    },
    {
        "id": "absence",
        "label": "No null",
        "title": "Absence, as a type",
        "point": "`Option<T>` is a library, not a keyword, and `match` forces both cases.",
        "files": ["examples/absence.bx"],
        "run": "examples/absence.bx",
    },
]

LANGS = {".bx": "burxt", ".php": "php", ".py": "python", ".rs": "rust"}


def read(path):
    with open(os.path.join(ROOT, path)) as f:
        return f.read().rstrip("\n")


def keep(text):
    """Everything the compiler said, minus its own "compiled X -> Y" line, which is noise here."""
    return "\n".join(l for l in text.splitlines() if not l.startswith("compiled ")).strip()


def run(path):
    """Compile and run a program, and classify how it ended.

    The three kinds are load-bearing rather than tidy: a scale mismatch is caught at COMPILE time,
    while an overflow is a well-typed program that STOPS. Calling the second a compile error would
    misdescribe how the language works.

    `-o` is not optional here. Without it `burxt run` writes the executable beside the source, so
    generating this page left five stray binaries in `examples/` — which is exactly what
    `the_repository_root_holds_only_what_belongs_there` exists to complain about one directory up.
    """
    work = tempfile.mkdtemp(prefix="burxt-example-")
    done = subprocess.run(
        [BURXT, "run", os.path.basename(path), "-o", os.path.join(work, "program")],
        cwd=os.path.join(ROOT, os.path.dirname(path)),
        capture_output=True, text=True, timeout=180,
    )
    if done.returncode == 0:
        return keep(done.stdout), "ok"
    shown = keep(done.stdout + done.stderr)
    return shown, ("runtime" if "burxt runtime error" in shown else "compile")


def schema_of(path):
    """`burxt mcp-schema`, pretty-printed so it can be read on a page."""
    done = subprocess.run(
        [BURXT, "mcp-schema", os.path.join(ROOT, path)],
        cwd=ROOT, capture_output=True, text=True, timeout=180,
    )
    if done.returncode != 0:
        sys.exit("burxt mcp-schema failed on %s:\n%s" % (path, done.stdout + done.stderr))
    return json.dumps(json.loads(done.stdout), indent=1)


def transcript(program, requests):
    """Build the server, pipe the requests through it, and keep what came back."""
    work = tempfile.mkdtemp(prefix="burxt-mcp-")
    server = os.path.join(work, "server")
    built = subprocess.run(
        [BURXT, "build", os.path.join(ROOT, program), "-o", server],
        cwd=ROOT, capture_output=True, text=True, timeout=600,
    )
    if built.returncode != 0:
        sys.exit("the MCP server did not build:\n%s%s" % (built.stdout, built.stderr))
    spoke = subprocess.run(
        [server], input="\n".join(requests) + "\n",
        capture_output=True, text=True, timeout=180,
    )
    return spoke.stdout.strip()


def build():
    panels = []
    for p in PROGRAMS:
        langs = {"Burxt": [{"name": os.path.basename(f), "path": f,
                            "lang": LANGS[os.path.splitext(f)[1]], "source": read(f)}
                           for f in p["files"]]}
        for label, files in p.get("ports", {}).items():
            langs[label] = [{"name": os.path.basename(f), "path": f,
                             "lang": LANGS[os.path.splitext(f)[1]], "source": read(f)}
                            for f in files]

        panel = {
            "id": p["id"], "label": p["label"], "title": p["title"], "point": p["point"],
            "langs": langs, "order": ["Burxt"] + list(p.get("ports", {}).keys()),
            "read": p.get("read", ""),
        }
        if p.get("run"):
            panel["output"], panel["kind"] = run(p["run"])
            panel["outputLabel"] = "Output, recorded by running it"
        elif p.get("schema"):
            panel["kind"] = "ok"
            panel["output"] = (
                "$ burxt mcp-schema " + p["schema"] + "\n" + schema_of(p["schema"]) +
                "\n\n$ " + os.path.basename(p["files"][-1]).replace(".bx", "") +
                "  < requests.jsonl\n" + transcript(p["files"][-1], p["requests"])
            )
            panel["outputLabel"] = "Derived, and answered, by the real thing"
        panels.append(panel)

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
description: Complete Burxt programs, and exactly what the compiler does with them — including the same till written in PHP, Python and Rust.
width: wide
---

# Examples

**Whole programs, not fragments.** Every result below came from compiling and running the program on
this page, and a test regenerates this file and fails if any of it has drifted — so nothing here can
claim something the compiler does not do.

Single rules are explained in [the guide]({{{{ site.baseurl }}}}/guide/), which now carries its own
`Examples` section on every page. This is for reading real code.

Start with the till. It is the same program four times, and the comparison is the argument.

<div class="picker" role="tablist">
{tabs}
</div>

<h2 data-title></h2>
<p id="point"></p>

<div class="picker" id="langs" role="tablist" aria-label="Language"></div>
<div class="picker" id="files" role="tablist" aria-label="File"></div>

<div id="code"></div>

<div class="out">
  <h4 id="outhead">Output</h4>
  <pre><code id="out"></code></pre>
  <p class="note" id="note"></p>
</div>

<p id="read" style="margin-top:1.5rem;"></p>

<p style="font-size:14px; margin-top:2rem;">
Every file above is in the repository — follow the filename to read it in context. To run any of them,
<a href="{{{{ site.baseurl }}}}/install/">install the compiler</a> or
<a href="https://codespaces.new/andrecorugda/burxt?quickstart=1">open a Codespace</a>, where it is a
click away.
</p>

<script>
const PANELS = {data};

const picker = document.querySelectorAll('.picker[role=tablist] > button[data-panel]');
const langs = document.getElementById('langs');
const files = document.getElementById('files');
const code = document.getElementById('code');
const out = document.getElementById('out');
const head = document.getElementById('outhead');
const note = document.getElementById('note');
const point = document.getElementById('point');
const title = document.querySelector('[data-title]');
const read = document.getElementById('read');

const HEAD = {{
  ok:      ['', ''],
  compile: ['Refused at compile time', 'stale'],
  runtime: ['Stopped at run time', 'stale'],
}};
const NOTE = {{
  ok:      'Recorded by running this program.',
  compile: 'The real message from the compiler, not a paraphrase.',
  runtime: 'It typechecks. It stops when the value cannot be represented.',
}};

let panel = PANELS[0];
let lang = 'Burxt';
let file = 0;

function md(text) {{
  // The one-line prose fields carry `code` spans and **strong**, and nothing else.
  return text
    .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
    .replace(/`([^`]+)`/g, '<code>$1</code>')
    .replace(/\\*\\*([^*]+)\\*\\*/g, '<strong>$1</strong>');
}}

function draw() {{
  const set = panel.langs[lang];
  if (file >= set.length) file = 0;
  const f = set[file];

  title.textContent = panel.title;
  point.innerHTML = md(panel.point);
  read.innerHTML = panel.read ? md(panel.read) : '';

  langs.hidden = panel.order.length < 2;
  langs.innerHTML = panel.order.map(name =>
    '<button role="tab" data-lang="' + name + '" aria-selected="' +
    (name === lang) + '">' + name + '</button>').join('');

  files.innerHTML = set.map((x, i) =>
    '<button role="tab" data-file="' + i + '" aria-selected="' + (i === file) + '">' +
    x.name + '</button>').join('');

  const escaped = f.source
    .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
  code.innerHTML = '<pre data-file="' + f.name + '"><code class="language-' + f.lang + '">' +
    escaped + '</code></pre>';
  // The panel was just replaced, so its code has to be enhanced again.
  if (window.BurxtEditor) window.BurxtEditor.enhance(code);

  out.textContent = panel.output || '';
  head.textContent = HEAD[panel.kind][0] || panel.outputLabel || 'Output';
  head.className = HEAD[panel.kind][1];
  note.textContent = lang === 'Burxt'
    ? NOTE[panel.kind]
    : 'This port prints the same thing. A test runs it and compares, and skips if ' + lang +
      ' is not installed.';

  picker.forEach(t => t.setAttribute('aria-selected', String(t.dataset.panel === panel.id)));
}}

picker.forEach(t => t.addEventListener('click', () => {{
  panel = PANELS.find(p => p.id === t.dataset.panel);
  lang = 'Burxt';
  file = 0;
  draw();
}}));

langs.addEventListener('click', e => {{
  const b = e.target.closest('button[data-lang]');
  if (!b) return;
  lang = b.dataset.lang;
  draw();
}});

files.addEventListener('click', e => {{
  const b = e.target.closest('button[data-file]');
  if (!b) return;
  file = Number(b.dataset.file);
  draw();
}});

draw();
</script>
'''.format(tabs=tabs, data=data)


if __name__ == "__main__":
    if not os.path.exists(BURXT):
        sys.exit("build the compiler first: cargo build --release  (or set BURXT=<path>)")
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
