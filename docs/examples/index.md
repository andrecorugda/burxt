---
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
  <button role="tab" data-panel="hello" aria-selected="true">Hello</button>
  <button role="tab" data-panel="money" aria-selected="false">Money</button>
  <button role="tab" data-panel="scales" aria-selected="false">Scales</button>
  <button role="tab" data-panel="overflow" aria-selected="false">Overflow</button>
  <button role="tab" data-panel="absence" aria-selected="false">No null</button>
  <button role="tab" data-panel="generics" aria-selected="false">Generics</button>
  <button role="tab" data-panel="maps" aria-selected="false">Maps</button>
  <button role="tab" data-panel="regions" aria-selected="false">Memory</button>
  <button role="tab" data-panel="contracts" aria-selected="false">Contracts</button>
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
const PANELS = [
 {
  "id": "hello",
  "label": "Hello",
  "point": "A whole program. No entry point to declare.",
  "source": "print(\"Hello, world!\");",
  "output": "Hello, world!",
  "kind": "ok"
 },
 {
  "id": "money",
  "label": "Money",
  "point": "Exact decimals are the default, computed as scaled integers.",
  "source": "let price: Decimal<2> = 19.99;\nlet qty:   Int        = 3;\nlet total: Decimal<2> = price * qty;\nprint(total);",
  "output": "59.97",
  "kind": "ok"
 },
 {
  "id": "scales",
  "label": "Scales",
  "point": "Adding decimals of different scales is a compile error, not a rounding.",
  "source": "let dollars: Decimal<2> = 19.99;\nlet precise: Decimal<4> = 0.0825;\nprint(dollars + precise);",
  "output": "error: cannot + Decimal<2> and Decimal<4>: scales must match. Burxt does not silently rescale money.\n --> snippet.bx:3:7\n  |\n3 | print(dollars + precise);\n  |       ^^^^^^^^^^^^^^^^^",
  "kind": "compile"
 },
 {
  "id": "overflow",
  "label": "Overflow",
  "point": "Arithmetic traps rather than wrapping around quietly.",
  "source": "let big: Int = 9223372036854775807;\nprint(big + 1);",
  "output": "burxt runtime error: arithmetic overflow \u2014 the exact result no longer fits in the value range",
  "kind": "runtime"
 },
 {
  "id": "absence",
  "label": "No null",
  "point": "Absence is a type, and both cases must be written.",
  "source": "use \"lib/option.bx\";\n\nfunction first_even(xs: [Int]) -> Option<Int> {\n    for x in xs {\n        if remainder(x, 2) == 0 {\n            return Option.Some(x);\n        }\n    }\n    return Option.None;\n}\n\nregion r {\n    let xs: [Int] = [3, 7, 8, 9];\n    match first_even(xs) {\n        None => { print(\"none\"); }\n        Some(n) => { print(n); }\n    }\n}",
  "output": "8",
  "kind": "ok"
 },
 {
  "id": "generics",
  "label": "Generics",
  "point": "One definition, one machine function per type. Nothing is erased.",
  "source": "function largest<T: Ordered>(a: T, b: T) -> T {\n    if a > b {\n        return a;\n    }\n    return b;\n}\n\nregion r {\n    print(largest(3, 9));\n    print(largest($2.50, $17.25));\n}",
  "output": "9\n17.25",
  "kind": "ok"
 },
 {
  "id": "maps",
  "label": "Maps",
  "point": "Iteration is insertion order, always. Never a hash order.",
  "source": "use \"lib/map.bx\";\n\nregion r {\n    let mutable counts: Map<String, Int> = map_new();\n    let a: Int = counts.set(\"pears\", 7);\n    let b: Int = counts.set(\"apples\", 3);\n    let c: Int = counts.set(\"plums\", 1);\n    let gone: Bool = counts.remove(\"apples\");\n\n    let names: [String] = counts.keys();\n    for name in names {\n        print(name);\n    }\n}",
  "output": "pears\nplums",
  "kind": "ok"
 },
 {
  "id": "regions",
  "label": "Memory",
  "point": "No collector. A region is a bump pointer and a mark.",
  "source": "function label(n: Int) -> String {\n    return \"item \" + to_string(n);\n}\n\nregion r {\n    let mutable i: Int = 1;\n    while i <= 3 {\n        print(label(i));\n        i += 1;\n    }\n}\n// every String built above is released here, at once, in O(1)",
  "output": "item 1\nitem 2\nitem 3",
  "kind": "ok"
 },
 {
  "id": "contracts",
  "label": "Contracts",
  "point": "A precondition is checked, and names itself when it fails.",
  "source": "function withdraw(balance: Decimal<2>, amount: Decimal<2>) -> Decimal<2>\n    requires amount > $0.00\n    requires amount <= balance\n    ensures result >= $0.00\n{\n    return balance - amount;\n}\n\nprint(withdraw($100.00, $30.00));\nprint(withdraw($100.00, $500.00));",
  "output": "70.00\nburxt runtime error: `requires amount <= balance` failed in `withdraw`",
  "kind": "runtime"
 }
];

const tabs = document.querySelectorAll('.picker button');
const src = document.getElementById('src');
const out = document.getElementById('out');
const head = document.getElementById('outhead');
const note = document.getElementById('note');
const point = document.getElementById('point');
let current = PANELS[0];

function show(panel) {
  current = panel;
  src.value = panel.source;
  out.textContent = panel.output;
  point.textContent = panel.point;
  const HEAD = {
    ok:      ['Output', ''],
    compile: ['Refused at compile time', 'stale'],
    runtime: ['Stopped at run time', 'stale'],
  };
  const NOTE = {
    ok:      'Recorded by running this program.',
    compile: 'The real message from the compiler, not a paraphrase.',
    runtime: 'It typechecks. It stops when the value cannot be represented.',
  };
  head.textContent = HEAD[panel.kind][0];
  head.className = HEAD[panel.kind][1];
  note.textContent = NOTE[panel.kind];
  const rows = panel.source.split('\n').length;
  src.style.minHeight = Math.max(14, rows + 3) * 1.55 + 2.2 + 'rem';
  tabs.forEach(t => t.setAttribute('aria-selected', String(t.dataset.panel === panel.id)));
}

tabs.forEach(t => t.addEventListener('click', () => {
  show(PANELS.find(p => p.id === t.dataset.panel));
}));

document.getElementById('copy').addEventListener('click', e => {
  e.preventDefault();
  navigator.clipboard.writeText(src.value).then(() => {
    const b = e.target;
    const was = b.textContent;
    b.textContent = 'Copied';
    setTimeout(() => { b.textContent = was; }, 1200);
  });
});

show(PANELS[0]);
</script>
