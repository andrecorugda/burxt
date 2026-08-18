//! `burxt fmt` — one layout, so a review is about the change.
//!
//! **Why this exists at all.** The point of Burxt is that an agent writes the code and a person
//! scans it. A reviewer who is arguing about where a brace goes is not reading the diff, and a diff
//! carrying a reformat is a diff nobody reads. A formatter is not a convenience here; it is the
//! thing that keeps review about meaning. `M10` §2e already made the case — *a formatter stops
//! style arguments in review, and review is the thesis.*
//!
//! It became load-bearing rather than nice when most Burxt stopped being typed by hand. BMX and
//! star-burxt generate it, and generated code is read in review far more often than it is written
//! and trusted least. If a reviewer cannot tell "a generator emitted this" from "a person wrote
//! this" by looking, that is a real cost — and a formatter makes it a non-question.
//!
//! # What it does, and deliberately nothing more
//!
//! Leading indentation, and trailing whitespace. That is all.
//!
//! It does **not** re-wrap lines, reorder anything, insert or remove blank lines, normalise spacing
//! inside a line, or touch a comment's text. Every one of those is a judgement about intent, and a
//! formatter that makes judgements is a formatter people turn off. The one thing nobody has an
//! opinion worth defending about is how far a line is indented, and that is the thing worth
//! settling mechanically.
//!
//! # Why a line-based formatter is SAFE here, and would not be in most languages
//!
//! **Burxt refuses a string that crosses a newline** — *"unterminated string literal — close it
//! with `"` before the end of the line"*. So no string interior spans lines, and rewriting a line's
//! leading whitespace cannot reach inside a literal. In a language with heredocs or triple-quoted
//! strings this approach would corrupt data, silently. Here the lexer has already made it
//! impossible, which is worth stating because the next person will wonder why this is not
//! reconstructing from the AST.
//!
//! Depth comes from the TOKENS rather than from counting characters, so a brace inside a string or
//! a comment cannot move anything. `//` inside a string is a string; the lexer says so and this
//! asks it rather than guessing.
//!
//! # Idempotence
//!
//! `format(format(x)) == format(x)`, and it is not a hope: the output's indentation is a pure
//! function of its token stream, and re-indenting changes no token. `tests/runner.rs` asserts it
//! over the whole corpus, and star-burxt's CI asserts it over generated components — which is the
//! harder test, because generated code is where a layout disagreement would actually show up.
//!
//! # The house style, which was uniform and unwritten
//!
//! Measured across 2,025 lines of `lib/` before this existed: **four spaces per level, no tabs
//! anywhere, contract clauses hanging one level under the signature with the brace on its own line
//! at signature depth, and a wrapped expression continuing one level in.** This file is now where
//! that is written down. It was previously nowhere — which is the same shape as the `lib/`
//! bare-filename convention and the 99 missing `pure` markers, both uniform-and-unwritten until
//! something enforced them.

use crate::diag::Diagnostic;
use crate::lexer::{Lexer, Token};

/// One level of indentation. Four spaces, because that is what the corpus already was.
const STEP: &str = "    ";

/// Apply one line's bracket operations to the open-bracket stack.
fn replay(stack: &mut Vec<bool>, ops: &[Option<bool>]) {
    for op in ops {
        match op {
            Some(inline) => stack.push(*inline),
            None => {
                stack.pop();
            }
        }
    }
}

/// Did this line leave a statement open, so the next one continues it?
///
/// **A trailing comma means two different things, and the difference is whether the bracket it sits
/// inside was the last token on ITS line.** A bracket ending a line opens a block, whose members sit
/// at block depth. A bracket with content after it opens something being written across lines, whose
/// continuation an author aligned by hand.
///
///     enum Json { Null, Truth(Bool) }          // brace mid-line: a literal, commas continue
///     enum Json {                              // brace last on line: a block, commas do not
///         Null,
///
/// Three separate corpus disagreements turned out to be this one distinction — enum variants gaining
/// a level, wrapped parameter lists sliding to column zero, and multi-line struct literals losing
/// their alignment. Guessing at any one of them in isolation produced a rule that broke the other
/// two.
///
/// Within parentheses it continues a wrapped parameter list, whose second line an author
/// aligned under the first:
///
///     function assert_money_equal(name: String, got: Decimal<2, RoundHalfEven>,
///                                 want: Decimal<2, RoundHalfEven>) -> Bool {
///
/// Within braces it ends a list item, which sits at block depth and nothing more:
///
///     enum Json { Null, Truth(Bool), Number(String) }
///
/// Treating the two alike broke both directions — every enum variant gained a level, and every
/// wrapped parameter list lost its alignment and slid to column zero. So `open_parens` is passed in:
/// a comma continues only while a `(` or `[` is still open.
///
/// A comment-only line carries no tokens and inherits what the code around it was doing, rather
/// than resetting it.
fn ends_open(last: &Option<Token>, carried: bool, inside_inline: bool) -> bool {
    match last {
        None => carried,
        // Any OPENER ending a line starts a block whose members sit at depth — the bracket has
        // already moved the depth, so calling the next line a continuation would double it. `{` was
        // handled and `[` and `(` were not, which indented every element of
        // `return html_element("li", [...], [` one level too far.
        Some(Token::Semicolon)
        | Some(Token::LBrace)
        | Some(Token::LBracket)
        | Some(Token::RBrace) => false,
        Some(Token::Comma) => inside_inline,
        Some(_) => true,
    }
}

/// Reformat `src`, or refuse with the lexer's own diagnostic.
///
/// **It refuses rather than guessing on a file that does not lex.** A formatter that reformats
/// broken source produces a file whose braces have moved and whose error now points somewhere else,
/// which turns one problem into two. `burxt fmt` on a file with a syntax error says so and changes
/// nothing.
pub fn format(src: &str) -> Result<String, Diagnostic> {
    let tokens = Lexer::new(src).tokenize()?;

    // Byte offset → line index, so a token can be attributed to the line it starts on.
    let mut line_of = Vec::with_capacity(src.len() + 1);
    let mut line = 0usize;
    for b in src.bytes() {
        line_of.push(line);
        if b == b'\n' {
            line += 1;
        }
    }
    line_of.push(line);

    let total = src.split('\n').count();
    let mut first: Vec<Option<Token>> = vec![None; total];
    let mut last: Vec<Option<Token>> = vec![None; total];
    // Net brace movement contributed by the tokens ON each line, and how far the line dips below
    // its own starting depth. `}` first on a line has to un-indent the line it is on, which needs
    // the dip rather than the net: `} else {` nets zero and still belongs one level out.
    let mut net: Vec<i32> = vec![0; total];
    let mut dip: Vec<i32> = vec![0; total];
    // For each line, the bracket-stack operations its tokens perform, in order: `Some(inline)` for
    // an opener that was (or was not) followed by more tokens on its own line, and `None` for a
    // closer. Replayed per line so `ends_open` can ask what the INNERMOST open bracket was.
    let mut brackets: Vec<Vec<Option<bool>>> = vec![Vec::new(); total];

    for (n, (tok, span)) in tokens.iter().enumerate() {
        let at = line_of[span.start as usize];
        // Whether the NEXT token is on this same line is what makes an opener inline.
        let next_line = tokens.get(n + 1).map(|(_, s)| line_of[s.start as usize]);
        if at >= total {
            continue;
        }
        if first[at].is_none() {
            first[at] = Some(tok.clone());
        }
        last[at] = Some(tok.clone());
        let step = match tok {
            Token::LBrace | Token::LBracket | Token::LParen => 1,
            Token::RBrace | Token::RBracket | Token::RParen => -1,
            _ => 0,
        };
        net[at] += step;
        if net[at] < dip[at] {
            dip[at] = net[at];
        }
        match tok {
            Token::LBrace | Token::LBracket | Token::LParen => {
                let inline = next_line.map(|l| l == at).unwrap_or(false);
                brackets[at].push(Some(inline));
            }
            Token::RBrace | Token::RBracket | Token::RParen => brackets[at].push(None),
            _ => {}
        }
    }

    let mut out = String::with_capacity(src.len());
    let mut depth: i32 = 0;
    // Set when the previous line left a statement open — no `;`, no brace. A contract clause is
    // exactly that case and needs no rule of its own: `pure function f(x: Int) -> Int` ends on a
    // type, so `requires x > 0` is a continuation and lands one level in, which is the house style
    // already in the corpus.
    let mut continued = false;
    let mut stack: Vec<bool> = Vec::new();

    for (i, raw) in src.split('\n').enumerate() {
        let body = raw.trim();
        if body.is_empty() {
            // A blank line stays blank rather than becoming indented whitespace. Trailing spaces on
            // an empty line are invisible in a review and show up in a diff, which is the wrong way
            // round.
            out.push('\n');
            continue;
        }

        // **A continuation is indented TWO levels, always.** One rule, no judgement, and the
        // second level is not decoration — at one level a continuation is indistinguishable from the
        // block body it precedes:
        //
        //     function f(a: Int,
        //         b: Int) -> Int {     <- continuation
        //         return a + b;        <- body, same column
        //
        // This replaces "keep whatever was written", which was the conservative choice and the wrong
        // one for this language. Alignment-to-the-bracket is what a person does by eye; it depends on
        // the width of the name to its left, so renaming a function reflows a paragraph. Preserving
        // it means the formatter permits two styles, and a formatter that permits two styles leaves
        // the argument in review — which is the thing it exists to remove. One spelling per concept
        // is the house rule; this is that rule applied to layout.
        let mut level = depth + dip[i];
        if continued {
            level += 2;
        }
        // ---- the one shape whose output reads oddest, reported from real use --------------------
        //
        // BMX, after formatting `burxt/bmx.bx` on 1.4.0: a multi-line struct literal inside a call
        // gets the continuation's +2 for its FIELDS, while the closing line dedents back to the
        // statement — so the block's opening and closing edges do not line up with each other:
        //
        //     let added: Int = push(out, Bmx.InlineBlock(BmxInline {
        //                 name: name,
        //                 head: substring(text, open + 1, shut - open - 1),
        //     }));
        //
        // It is consistent and it is not wrong: `dip` dedents the `}` line by one while `continued`
        // added two, and every file in the corpus agrees. **Left as it is on purpose.** Changing the
        // rule reflows the whole corpus in BOTH compilers and the differential that holds them
        // byte-identical, which is a large change for a cosmetic gain — and the corpus test is what
        // makes the rule trustworthy, so churning it to taste is the wrong trade.
        //
        // Recorded here rather than in a tracker because whoever revisits the continuation rule
        // reads this function, and this is the case that should be tried against a new one first.
        // BMX's own verdict: "consistent, not wrong, and I have taken it rather than fighting it."

        if level < 0 {
            level = 0;
        }

        for _ in 0..level {
            out.push_str(STEP);
        }
        out.push_str(body);
        out.push('\n');

        depth += net[i];
        if depth < 0 {
            depth = 0;
        }
        replay(&mut stack, &brackets[i]);
        continued = ends_open(&last[i], continued, *stack.last().unwrap_or(&false));
    }

    // `split('\n')` yields a trailing empty piece for a file ending in a newline, and the loop
    // above already emitted its `\n`. Drop the duplicate rather than growing the file by one line
    // every time this runs — which would have made it the opposite of idempotent.
    if src.ends_with('\n') && out.ends_with("\n\n") {
        out.pop();
    }
    Ok(out)
}
