---
title: The guide
section: guide
description: Twelve pages in reading order. Each explains a decision and the reasoning behind it.
---

# The guide

Twelve pages, in reading order. Each one explains a **decision** and the reasoning behind it — what
the language refuses is usually the interesting part.

<ul class="pages">
<li><a href="01-getting-started.html"><span class="n">1</span> <span>Getting started</span> <span class="what">Install, run a file, the editor</span></a></li>
<li><a href="02-numbers-and-money.html"><span class="n">2</span> <span>Numbers and money</span> <span class="what">Scales, rounding contracts, why <code>+</code> is strict</span></a></li>
<li><a href="03-types.html"><span class="n">3</span> <span>Types</span> <span class="what">Classes, <code>private</code>, constructors, interfaces, enums</span></a></li>
<li><a href="04-memory.html"><span class="n">4</span> <span>Memory</span> <span class="what">Regions, escapes, and why you never write <code>allocates</code></span></a></li>
<li><a href="05-contracts.html"><span class="n">5</span> <span>Contracts</span> <span class="what"><code>requires</code>, <code>ensures</code>, <code>pure</code>, <code>decreases</code></span></a></li>
<li><a href="06-effects.html"><span class="n">6</span> <span>Effects</span> <span class="what"><code>touches files, network</code> — what a function can reach</span></a></li>
<li><a href="07-ffi.html"><span class="n">7</span> <span>The C boundary</span> <span class="what"><code>external function</code>, <code>as scaled</code>, the pointer wall</span></a></li>
<li><a href="08-modules.html"><span class="n">8</span> <span>Modules</span> <span class="what"><code>use</code>, one file per module, what is visible</span></a></li>
<li><a href="09-generics.html"><span class="n">9</span> <span>Generics</span> <span class="what">Type parameters, bounds, why nothing is erased</span></a></li>
<li><a href="10-absence-and-failure.html"><span class="n">10</span> <span>Absence and failure</span> <span class="what"><code>Option</code>, <code>Result</code>, <code>?</code>, and no null</span></a></li>
<li><a href="11-maps.html"><span class="n">11</span> <span>Maps</span> <span class="what">Insertion order, <code>Equatable</code> keys, <code>get</code> and <code>find</code></span></a></li>
<li><a href="reference.html"><span class="n">—</span> <span>Reference</span> <span class="what">Every keyword, builtin, operator and error</span></a></li>
</ul>

Running code beats prose: the [examples]({{ site.baseurl }}/examples/) page shows programs beside
exactly what the compiler does with them, and every one of those results was recorded by running it.
