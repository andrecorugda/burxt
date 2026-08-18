/* Turn every Burxt code block on the site into what it looks like in an editor.
 *
 * The site used to render 92 code blocks as plain monospace text on a grey wash: no colour, no line
 * numbers, and compiler errors sitting in a separate box with no connection to the code they point
 * at. `docs/_config.yml` even claimed "the site does its own colouring for them", which was a
 * description of an intention rather than of a shipped feature.
 *
 * Three things happen here.
 *
 *   1. A macOS-window panel: a title bar naming the file, and a line-number gutter that rides the
 *      horizontal scroll and never joins a selection, so copying code gives you code.
 *
 *   2. Xcode's light syntax colours, from word lists that mirror `src/lexer.rs` — so a snippet on
 *      this page and the same snippet in the VS Code extension are the same colours.
 *      `the_web_highlighter_knows_every_keyword_the_compiler_does` in tests/runner.rs reads the
 *      compiler's own keyword table and fails if this file has fallen behind it.
 *
 *   3. THE SQUIGGLE, which is the point. Where the page shows a refusal, the compiler's own caret
 *      rendering is parsed and turned into a wavy underline on exactly the columns it marked, with
 *      the real message underneath it. Nothing here invents a diagnostic: every one is text a
 *      generator captured by running the compiler, and the underline lands where `src/diag.rs` said.
 *
 * Progressive enhancement, deliberately. The markdown fences are untouched, which is what lets
 * `the_guide_code_compiles` go on compiling all 92 of them — a snippet on this site cannot drift
 * from what the compiler does, because the snippet IS what the test compiles. With scripting off
 * the page is still a document with readable code in it.
 *
 * There is no editing. Without a WebAssembly build there is no compiler here, so a typeable panel
 * could only ever show a stale squiggle over code the reader had just changed. */
(function () {
  'use strict';

  /* ---- the words -------------------------------------------------------------------------------
   *
   * Every list below is the compiler's, not a guess. `src/lexer.rs`'s keyword table for the first
   * four, `renamed_keyword` for the refused spellings, `is_reserved_name` in `src/typeck.rs` for the
   * builtins, and `src/ast.rs` for the effects. */

  // The declaration and statement keywords, plus the literals.
  var KEYWORD = words(
    'let const mutable function external return as tail break continue if else while class private ' +
    'region enum match interface is implement implements for in dynamic print print_error true false pure public'
  );

  // Types the language owns. Anything else Capitalised is a type the program declared.
  // `i32 u8 u32 u64` are the sized C integers (roadmap A7) and belong here, not in the Rust word
  // list further down that happens to contain the same spellings — the site highlights Rust blocks
  // too, and matching them there colours Rust code while leaving Burxt code plain.
  var TYPE = words('Int Bool String CInt CDouble CPointer Decimal RoundHalfEven RoundHalfUp '
    + 'i32 u8 u32 u64');

  // The contract vocabulary. Contextual in the parser — legal as ordinary names elsewhere — but on
  // a signature they are the whole reason this language exists, so they get their own colour.
  var CONTRACT = words('requires ensures decreases touches allocates');

  // The six effects a `touches` clause may name. Coloured only on a line that says `touches`,
  // because `files` is an ordinary variable name everywhere else.
  var EFFECT = words('files commands clock input network model');

  // Values the language puts in scope for you.
  var LANG = words('self result it');

  // Built-in calls. `old` is one too: it is spelled like a call and means "before the body ran".
  var BUILTIN = words(
    'len byte_at byte_as_string handle_of handle_value push read_file to_string old substring truncate write_file write_bytes ' +
    'argument argument_count divide_floor divide_toward_zero remainder hash exit ' +
    'bit_and bit_or bit_xor bit_not shift_left shift_right_zeros shift_right_sign ' +
    'c_is_null c_string_at c_bytes_at c_bytes_to'
  );

  // Spellings that do not compile. The eight the language renamed, and `main` — Burxt has no entry
  // point, so a `function main()` looks like one and is not one. Coloured as the errors they are,
  // which is what the editor grammar does too.
  var REFUSED = words('fn mut impl dyn extern struct trait record main');

  function words(s) {
    var set = Object.create(null);
    s.split(' ').forEach(function (w) { if (w) set[w] = true; });
    return set;
  }

  function esc(s) {
    return s.replace(/[&<>]/g, function (c) {
      return c === '&' ? '&amp;' : c === '<' ? '&lt;' : '&gt;';
    });
  }
  function span(cls, text) { return '<span class="' + cls + '">' + esc(text) + '</span>'; }

  /* ---- the tokenizer ---------------------------------------------------------------------------
   *
   * One line at a time, which is correct rather than merely convenient: Burxt has line comments
   * only — `/* *\/` is a dedicated error — and a string literal may not span lines. So no state
   * crosses a newline, and there is no block-comment mode to get wrong. */

  var WORD = /^[A-Za-z_][A-Za-z0-9_]*/;

  function line(src) {
    var out = '';
    var i = 0;
    var touches = src.indexOf('touches') >= 0;

    while (i < src.length) {
      var rest = src.slice(i);
      var ch = rest.charAt(0);

      // A comment runs to the end of the line, and nothing inside it is code.
      if (rest.slice(0, 2) === '//') { out += span('t-com', rest); break; }

      // A string, with its escapes and its `{expr}` interpolations.
      if (ch === '"') { var s = string(rest); out += s.html; i += s.length; continue; }

      // `$19.99` — money. Its own colour: it is the language's signature literal, and the `$` is
      // documentation rather than arithmetic.
      var m = /^\$\d+(\.\d+)?/.exec(rest);
      if (m) { out += span('t-money', m[0]); i += m[0].length; continue; }

      // `8.25%` — a percent, which is a Decimal two scales finer. Also money-coloured: the point of
      // both literals is that you can see the exactness in the source.
      m = /^\d+(\.\d+)?%/.exec(rest);
      if (m) { out += span('t-money', m[0]); i += m[0].length; continue; }

      m = /^\d+(\.\d+)?/.exec(rest);
      if (m) { out += span('t-num', m[0]); i += m[0].length; continue; }

      m = WORD.exec(rest);
      if (m) {
        var w = m[0];
        // A call, or a declared name: `open(` and `function open`. Checked before the generic
        // lower-case case so a method reads as a method.
        var after = rest.slice(w.length);
        var called = /^\s*\(/.test(after);
        out += span(classify(w, called, touches), w);
        i += w.length;
        continue;
      }

      // Operators, then everything else. Grouped so `->` and `=>` are one token rather than two,
      // which matters because they are single tokens to the lexer.
      m = /^(->|=>|==|!=|<=|>=|&&|\|\||\+=|-=|\*=|[-+*\/=<>!?])/.exec(rest);
      if (m) { out += span('t-punc', m[0]); i += m[0].length; continue; }

      out += esc(ch);
      i += 1;
    }
    return out;
  }

  /* ---- the other three languages ---------------------------------------------------------------
   *
   * The examples page shows the same point-of-sale program in Burxt, PHP, Python and Rust, because
   * the comparison is the argument: the Burxt column is the only one where the rounding rule is in a
   * type rather than in an argument somebody has to remember to pass.
   *
   * That comparison only works if all four are readable, so the ports get colour too. Not a full
   * grammar each — keywords, types, strings, numbers and comments, which is what you need to read a
   * 90-line file. Anything more would be three more things to maintain for a page that exists to
   * make one point. */

  var PORTS = {
    php: {
      comments: ['//', '#'],
      quotes: '"\'',
      keyword: words(
        'abstract and array as break callable case catch class clone const continue declare ' +
        'default do echo else elseif empty enddeclare endfor endforeach endif endswitch endwhile ' +
        'enum extends final finally fn for foreach function global if implements include ' +
        'include_once instanceof insteadof interface isset list match namespace new or print ' +
        'private protected public readonly require require_once return static switch throw trait ' +
        'try unset use var while xor yield true false null'
      ),
      type: words('int float string bool array void mixed self static callable iterable object')
    },
    python: {
      comments: ['#'],
      quotes: '"\'',
      keyword: words(
        'and as assert async await break class continue def del elif else except finally for ' +
        'from global if import in is lambda nonlocal not or pass raise return try while with ' +
        'yield True False None match case'
      ),
      type: words('int float str bool list dict tuple set bytes Decimal Optional')
    },
    rust: {
      comments: ['//'],
      quotes: '"',
      keyword: words(
        'as async await break const continue crate dyn else enum extern false fn for if impl in ' +
        'let loop match mod move mut pub ref return self Self static struct super trait true type ' +
        'unsafe use where while'
      ),
      type: words(
        'i8 i16 i32 i64 i128 isize u8 u16 u32 u64 u128 usize f32 f64 bool char str String Vec ' +
        'Option Result Box HashMap Decimal'
      )
    }
  };

  function generic(src, cfg) {
    var out = '';
    var i = 0;
    while (i < src.length) {
      var rest = src.slice(i);

      var comment = null;
      for (var c = 0; c < cfg.comments.length; c++) {
        if (rest.slice(0, cfg.comments[c].length) === cfg.comments[c]) comment = cfg.comments[c];
      }
      if (comment) { out += span('t-com', rest); break; }

      var ch = rest.charAt(0);
      if (cfg.quotes.indexOf(ch) >= 0) {
        // Up to the next unescaped quote of the same kind, or the end of the line.
        var j = 1;
        while (j < rest.length) {
          if (rest.charAt(j) === '\\') { j += 2; continue; }
          if (rest.charAt(j) === ch) { j += 1; break; }
          j += 1;
        }
        out += span('t-str', rest.slice(0, j));
        i += j;
        continue;
      }

      var m = /^\d[\d_]*(\.\d+)?/.exec(rest);
      if (m) { out += span('t-num', m[0]); i += m[0].length; continue; }

      // PHP's `$total` is one token, and colouring the sigil separately reads as noise.
      m = /^\$?[A-Za-z_][A-Za-z0-9_]*/.exec(rest);
      if (m) {
        var w = m[0];
        var bare = w.replace(/^\$/, '');
        var cls = 't-id';
        if (cfg.keyword[bare]) cls = 't-kw';
        else if (cfg.type[bare]) cls = 't-type';
        else if (/^[A-Z]/.test(bare)) cls = 't-type';
        else if (/^\s*\(/.test(rest.slice(w.length))) cls = 't-fn';
        out += span(cls, w);
        i += w.length;
        continue;
      }

      m = /^(->|=>|::|==|!=|<=|>=|&&|\|\||\+=|-=|\*=|[-+*\/=<>!?])/.exec(rest);
      if (m) { out += span('t-punc', m[0]); i += m[0].length; continue; }

      out += esc(ch);
      i += 1;
    }
    return out;
  }

  function classify(w, called, touches) {
    if (REFUSED[w]) return 't-bad';
    if (CONTRACT[w]) return 't-contract';
    if (LANG[w]) return 't-lang';
    if (KEYWORD[w]) return 't-kw';
    if (TYPE[w]) return 't-type';
    if (BUILTIN[w]) return 't-fn';
    if (touches && EFFECT[w]) return 't-contract';
    // A Capitalised name is a type, a class, an interface or an enum. Burxt's naming rule makes
    // that reliable enough to colour on: `spec/A7.0-NAMING.md`.
    if (/^[A-Z]/.test(w)) return 't-type';
    if (called) return 't-fn';
    return 't-id';
  }

  function string(rest) {
    // Returns the coloured HTML for one string literal and how many characters it consumed.
    var html = '<span class="t-str">"';
    var i = 1;
    while (i < rest.length) {
      var ch = rest.charAt(i);
      if (ch === '\\') {
        // The escapes the lexer accepts, and nothing else. `\r` and `\0` arrived in v0.0.176.
        var pair = rest.substr(i, 2);
        var ok = /^\\[nrt0"\\{}]$/.test(pair);
        html += '</span>' + span(ok ? 't-num' : 't-bad', pair) + '<span class="t-str">';
        i += 2;
        continue;
      }
      if (ch === '"') { html += '"</span>'; return { html: html, length: i + 1 }; }
      if (ch === '{') {
        // `"total: {amount}"` — the inside is code, and the compiler treats it as such.
        var close = rest.indexOf('}', i);
        if (close < 0) { html += esc(rest.slice(i)); i = rest.length; break; }
        html += '</span>' + span('t-punc', '{') + line(rest.slice(i + 1, close)) +
                span('t-punc', '}') + '<span class="t-str">';
        i = close + 1;
        continue;
      }
      html += esc(ch);
      i += 1;
    }
    // An unterminated literal. The compiler refuses it; showing it plainly is honest.
    html += '</span>';
    return { html: html, length: i };
  }

  /* ---- the compiler's own output ---------------------------------------------------------------
   *
   * Parsed, never invented. `src/diag.rs` renders:
   *
   *     error: {message}
   *      --> {path}:{line}:{column}
   *       |
   *     5 | let total: Decimal<2> = price + rate;
   *       |                         ^^^^^^^^^^^^
   *
   * The line and the column come off the `-->` line rather than being counted out of the caret
   * row's indentation, because the arrow states them outright and the gutter's width changes with
   * the line number — at line 10 that indentation grows by one and a hand-counted offset is wrong. */

  function diagnostics(text) {
    var found = [];

    // A runtime failure has no caret and no location: it is a well-typed program that stopped.
    var trap = /burxt runtime error:.*/.exec(text);

    text.split(/\n(?=error: )/).forEach(function (block) {
      if (block.indexOf('error: ') !== 0) return;
      var lines = block.split('\n');
      var message = [];
      var at = null;
      var width = 1;

      for (var i = 0; i < lines.length; i++) {
        var l = lines[i];
        var arrow = /^\s*-->\s+(.+):(\d+):(\d+)\s*$/.exec(l);
        if (arrow) {
          at = { file: arrow[1], line: +arrow[2], column: +arrow[3] };
          continue;
        }
        var carets = /^\s*\|\s*(\^+)\s*$/.exec(l);
        if (carets) { width = carets[1].length; continue; }
        // The gutter rows and the echoed source line are the compiler's own rendering of code this
        // page is already showing, so they are not part of the message.
        if (/^\s*\d*\s*\|/.test(l)) continue;
        if (at) continue;                       // anything after the arrow belongs to the frame
        message.push(l.replace(/^error: /, '').trim());
      }

      found.push({
        kind: 'error',
        message: message.join(' ').replace(/\s+/g, ' ').trim(),
        at: at,
        width: width
      });
    });

    if (!found.length && trap) {
      found.push({
        kind: 'runtime',
        message: trap[0].replace(/^burxt runtime error:\s*/, '').trim(),
        at: null,
        width: 0
      });
    }
    return found;
  }

  /* ---- building the panel ---------------------------------------------------------------------- */

  // What the title bar calls an unnamed file, per language.
  var EXT = { burxt: '.bx', php: '.php', python: '.py', rust: '.rs' };

  function paint(text, lang) {
    return PORTS[lang] ? generic(text, PORTS[lang]) : line(text);
  }

  function build(pre, code, output, lang) {
    var source = code.textContent.replace(/\n$/, '');
    var rows = source.split('\n');
    var diags = output ? diagnostics(output.text) : [];

    // The filename the compiler itself used, when there is one. Better than a guess, and it tells
    // the reader this output came from a real file rather than from prose.
    var named = null;
    diags.forEach(function (d) { if (!named && d.at) named = d.at.file.split('/').pop(); });
    var file = pre.getAttribute('data-file') || named ||
      (pre.closest('.hero') ? 'account.bx' : 'main' + (EXT[lang] || '.bx'));

    var kind = diags.length ? diags[0].kind : null;

    // Which columns to underline, per line. A line can carry more than one.
    var marks = {};
    diags.forEach(function (d) {
      if (!d.at || !d.width) return;
      (marks[d.at.line] = marks[d.at.line] || []).push({ from: d.at.column, width: d.width, d: d });
    });

    var html = '';
    rows.forEach(function (text, n) {
      var no = n + 1;
      var here = marks[no];
      var body;
      var m = here && here.length ? here[0] : null;
      var from = m ? Math.max(0, m.from - 1) : 0;
      var to = m ? Math.min(text.length, from + m.width) : 0;
      // A range that covers nothing means the shown snippet and the quoted error disagree about
      // line numbers — an author trimmed a comment, say. Underlining zero characters would draw an
      // invisible marker and look like the feature silently not working, so fall back to the
      // message alone, which is still true.
      if (m && to > from) {
        // Underline exactly the run the carets covered, and colour each part properly — the
        // squiggle wraps coloured tokens rather than replacing them.
        body = paint(text.slice(0, from), lang) +
               '<span class="sq">' + paint(text.slice(from, to), lang) + '</span>' +
               paint(text.slice(to), lang);
      } else {
        body = paint(text, lang);
      }
      html += '<div class="cl' + (here ? ' bad' : '') + '">' +
              '<span class="lnum">' + no + '</span>' +
              '<code class="ltext">' + body + '</code></div>';

      // The message, on the row under the line it belongs to — where an editor puts it.
      if (here) {
        here.forEach(function (m) {
          html += '<div class="diag"><span class="diag-kind">error</span>' +
                  '<span>' + esc(m.d.message) + '</span></div>';
        });
      }
    });

    // A runtime failure belongs at the end: the program ran, printed, and then stopped.
    diags.forEach(function (d) {
      if (d.at || !d.message) return;
      html += '<div class="diag"><span class="diag-kind">' +
              (d.kind === 'runtime' ? 'stopped' : 'error') + '</span>' +
              '<span>' + esc(d.message) + '</span></div>';
    });

    var panel = document.createElement('div');
    panel.className = 'code';
    panel.innerHTML =
      '<div class="code-bar">' +
        '<span class="code-file">' + esc(file) + '</span>' +
        (kind ? '<span class="code-tag ' + (kind === 'runtime' ? 'runtime' : 'refused') + '">' +
                (kind === 'runtime' ? 'stops' : 'refused') + '</span>' : '') +
        '<button class="code-copy" type="button">Copy</button>' +
      '</div>' +
      '<div class="code-scroll"><div class="code-lines">' + html + '</div></div>';

    // The compiler's raw text, kept and quotable. Collapsed, because the squiggle above has already
    // said it — but a reader who wants to paste the exact message into an issue must be able to.
    if (output) {
      var out = document.createElement('details');
      out.className = 'code-out';
      out.innerHTML = '<summary>The compiler\'s output, verbatim</summary>';
      var box = document.createElement('pre');
      box.textContent = output.text;
      out.appendChild(box);
      panel.appendChild(out);
      output.pre.remove();
      if (output.lead) output.lead.remove();
    }

    panel.querySelector('.code-copy').addEventListener('click', function (e) {
      var b = e.currentTarget;
      var done = function () {
        b.textContent = 'Copied';
        b.setAttribute('data-done', '1');
        setTimeout(function () { b.textContent = 'Copy'; b.removeAttribute('data-done'); }, 1400);
      };
      if (navigator.clipboard) navigator.clipboard.writeText(source).then(done, function () {});
      else done();
    });

    // If the block came wrapped, the wrapper goes too — otherwise the finished panel sits inside an
    // empty bordered box that rouge drew.
    var host = pre.closest('div.highlighter-rouge') || pre;
    host.replaceWith(panel);
  }

  /* ---- pairing a refusal with its message -----------------------------------------------------
   *
   * The same rule `the_guide_code_compiles` uses to decide that a block is shown as REFUSED: the
   * error follows it almost immediately. "Almost" is load-bearing — without a distance limit this
   * matched a perfectly good `class Account` on guide page 3 that is followed by prose and THEN an
   * error about a different snippet. A lead-in of a few words, and nothing more. */

  /* Two shapes of code block reach the browser, and this cost the squiggles a deploy to find.
   *
   * `highlighter: rouge` is set, and rouge lexes a fence it recognises — a bare ``` is `plaintext` —
   * into a wrapper:
   *
   *     DIV.language-plaintext.highlighter-rouge > DIV.highlight > PRE.highlight > CODE
   *
   * `burxt` is not a language rouge knows, so kramdown falls back to emitting a bare
   * `PRE > CODE.language-burxt` with no wrapper at all. So on every refusal on the site the code
   * block is a bare `<pre>` and its error block is three levels deeper — which means the error was
   * never `pre.nextElementSibling`, and every squiggle silently did not happen.
   *
   * It looked right locally because the harness it was built against reproduced the shape this file
   * assumed rather than the shape Jekyll emits. There is no Ruby on the machine, so the only place
   * that difference was visible was the live site. */

  function unwrap(node) {
    // The <pre> a following sibling holds, whether it IS one or merely wraps one.
    if (!node) return null;
    if (node.tagName === 'PRE') return node;
    if (node.tagName === 'DIV' && /highlight/.test(String(node.className))) {
      return node.querySelector('pre');
    }
    return null;
  }

  function outputFor(pre) {
    var lead = null;
    // A wrapped code block's siblings are the WRAPPER's siblings, not the <pre>'s.
    var from = pre.closest('div.highlighter-rouge') || pre;
    var node = from.nextElementSibling;
    for (var hops = 0; node && hops < 2; hops++) {
      var found = unwrap(node);
      if (found) {
        var text = found.textContent.replace(/\s+$/, '');
        var isDiag = /^error: /.test(text) || text.indexOf('burxt runtime error') >= 0;
        // A `burxt` block is the next EXAMPLE, not this one's output.
        var isCode = found.querySelector('code.language-burxt');
        // `node`, not `found`: removing the <pre> out of a rouge wrapper would leave the wrapper
        // behind as an empty bordered box.
        if (isDiag && !isCode) return { pre: node, text: text, lead: lead };
        return null;
      }
      // One short lead-in — "Refused at compile time:" — may sit between them.
      if (node.textContent.trim().length >= 60) return null;
      lead = node;
      node = node.nextElementSibling;
    }
    return null;
  }

  /* ---- go --------------------------------------------------------------------------------------
   *
   * `code.language-burxt` is what kramdown emits for a ```burxt fence. The hero on the landing page
   * is a hand-written <pre><code> with no class — a fence does not render inside its div, and
   * `the_landing_page_code_compiles` splits the file on that exact literal string, so it must NOT
   * gain one. It is matched by position instead.
   *
   * `enhance` takes a root so that a page which swaps panels in — the examples page switches between
   * four languages — can have the new ones done too. It is idempotent: a panel already built is
   * skipped, because the <pre> it came from is gone. */

  function langOf(code) {
    var m = /language-([a-z]+)/.exec(code.className || '');
    var name = m ? m[1] : 'burxt';
    return PORTS[name] ? name : 'burxt';
  }

  function enhance(root) {
    root = root || document;
    var blocks = [].slice.call(root.querySelectorAll(
      'pre > code.language-burxt, pre > code.language-php, pre > code.language-python, ' +
      'pre > code.language-rust, .hero pre > code'
    ));
    blocks.forEach(function (code) {
      var pre = code.parentElement;
      if (!pre || pre.closest('.code')) return;          // already inside a panel
      build(pre, code, outputFor(pre), langOf(code));
    });
  }

  window.BurxtEditor = { enhance: enhance };
  enhance(document.querySelector('main') || document);
})();
