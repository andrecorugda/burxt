//! `burxt review old.bx new.bx` — what changed about what the program PROMISES.
//!
//! This is not a diff. A diff shows you every line that moved; this shows you only the lines
//! that changed a guarantee, and names which guarantee.
//!
//! ## Why it exists
//!
//! `DESIGN.md` states the purpose: an agent writes the code and a senior developer reviews it.
//! Under that arrangement the most dangerous change anyone can make is a **weakened contract** —
//! an agent that cannot satisfy `requires amount <= self.balance` deletes it. That passes every
//! test, because the tests were failing *because of* the contract. And **no tool in any other
//! language can flag it**, for one structural reason: everywhere else the assertion is a line in
//! a body, indistinguishable from any other line. Here it is part of the signature.
//!
//! So the claim this file makes good on is narrow and checkable:
//!
//! > A change that alters no signature cannot alter what the program promises.
//!
//! ## What it reads
//!
//! Declarations only — the lexer and the parser, no typechecker. Three consequences, all wanted:
//! it is fast; it does not care whether the program's bodies are finished; and it cannot be
//! confused by a reformat, because it compares meanings rather than text.
//!
//! ## Exit codes, because this is meant to be a gate
//!
//! `0` nothing weakened · `1` something weakened · `2` a file could not be read or parsed. So
//! `burxt review a.bx b.bx` in CI needs no output parsing.
//!
//! ## One real limitation
//!
//! Both versions must parse with the CURRENT compiler, so a change that spans a keyword rename
//! cannot be reviewed — `trait` was renamed to `interface` in v0.0.153, and today's binary
//! refuses the old spelling by design. Found by pointing this at its own repository's history.
//! Reviewing across a rename would need the old compiler, which is a different tool.
//!
//! ## Effects
//!
//! Since v0.0.159 a signature says what it `touches` — files, commands, clock, input, network,
//! model — so "this function now talks to the network" is a WEAKENED finding. That works only
//! because effects are DECLARED rather than inferred: an agent cannot add one without changing a
//! signature, and a signature change is what a reviewer reads.

use crate::ast::{Contract, Effect, Expr, ExprKind, Param, Program, Type};
use crate::lexer::Lexer;
use crate::parser::Parser;
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// Everything one declaration promises, flattened into something comparable.
///
/// A `BTreeMap<String, Promise>` keyed by qualified name is the whole data structure: `Account`,
/// `Account.balance`, `Account.withdraw`. Sorted, so output order is stable and a reviewer
/// reading two runs is not also diffing the order.
#[derive(PartialEq)]
struct Promise {
    /// `function`, `method`, `constructor`, `class`, `field`, `interface`, `const`.
    kind: &'static str,
    /// The parameter and return types, rendered. Not the body, and not the parameter NAMES —
    /// renaming a parameter changes no promise, and reporting it would be noise that teaches a
    /// reviewer to stop reading.
    shape: String,
    requires: Vec<String>,
    ensures: Vec<String>,
    is_pure: bool,
    private: bool,
    /// What this reaches outside itself. GAINING one is the second half of what a reviewer needs
    /// — "this function now talks to the network" is a change in what the program can do, and
    /// since v0.0.159 it cannot happen without the signature changing.
    touches: Vec<Effect>,
}

/// One thing worth telling a reviewer, and how much it should worry them.
struct Finding {
    verdict: &'static str,
    name: String,
    detail: String,
}

/// Weakened means "this promises LESS than it did", and it is the only verdict that should stop a
/// review. Everything else is information.
const WEAKENED: &str = "WEAKENED";
const STRICTER: &str = "STRICTER";
const ADDED: &str = "ADDED";
const REMOVED: &str = "REMOVED";
const CHANGED: &str = "CHANGED";

pub fn review(old_path: &str, new_path: &str) -> Result<i32, String> {
    let before = promises_of(old_path)?;
    let after = promises_of(new_path)?;

    let mut findings = Vec::new();
    for (name, was) in &before {
        match after.get(name) {
            None => findings.push(Finding {
                // A removed declaration is a weakening only if something still calls it, which
                // this cannot know — so it is reported plainly and left to the reviewer.
                verdict: REMOVED,
                name: name.clone(),
                detail: format!("{} is gone", was.kind),
            }),
            Some(now) => compare(name, was, now, &mut findings),
        }
    }
    for (name, now) in &after {
        if !before.contains_key(name) {
            findings.push(Finding {
                verdict: ADDED,
                name: name.clone(),
                detail: format!("new {}{}", now.kind, if now.private { ", private" } else { "" }),
            });
        }
    }

    // Weakenings first. A reviewer reads the top of the output.
    let rank = |v: &str| match v {
        WEAKENED => 0,
        CHANGED => 1,
        REMOVED => 2,
        STRICTER => 3,
        _ => 4,
    };
    findings.sort_by(|a, b| rank(a.verdict).cmp(&rank(b.verdict)).then(a.name.cmp(&b.name)));

    if findings.is_empty() {
        println!("no promise changed.");
        println!();
        println!("Every signature is identical, so whatever moved in the bodies, this program");
        println!("guarantees exactly what it guaranteed before.");
        return Ok(0);
    }

    let weakened = findings.iter().filter(|f| f.verdict == WEAKENED).count();
    for f in &findings {
        println!("{:<9} {:<34} {}", f.verdict, f.name, f.detail);
    }
    println!();
    if weakened > 0 {
        println!(
            "{} weakened promise(s). A weakened contract is the one change that passes every \
             test — the tests were failing BECAUSE of it.",
            weakened
        );
        // Non-zero, so this is usable as a gate in CI without parsing the output.
        return Ok(1);
    }
    println!("{} change(s), none weakening.", findings.len());
    Ok(0)
}

fn compare(name: &str, was: &Promise, now: &Promise, out: &mut Vec<Finding>) {
    // A contract that was there and is not is THE finding this tool exists for.
    for clause in &was.requires {
        if !now.requires.iter().any(|c| same_clause(c, clause)) {
            out.push(Finding {
                verdict: WEAKENED,
                name: name.to_string(),
                detail: format!("lost `requires {}`", shown(clause)),
            });
        }
    }
    for clause in &was.ensures {
        if !now.ensures.iter().any(|c| same_clause(c, clause)) {
            out.push(Finding {
                verdict: WEAKENED,
                name: name.to_string(),
                detail: format!("lost `ensures {}`", shown(clause)),
            });
        }
    }
    for clause in &now.requires {
        if !was.requires.iter().any(|c| same_clause(c, clause)) {
            out.push(Finding {
                verdict: STRICTER,
                name: name.to_string(),
                detail: format!("gained `requires {}`", shown(clause)),
            });
        }
    }
    for clause in &now.ensures {
        if !was.ensures.iter().any(|c| same_clause(c, clause)) {
            out.push(Finding {
                verdict: STRICTER,
                name: name.to_string(),
                detail: format!("gained `ensures {}`", shown(clause)),
            });
        }
    }
    // An effect GAINED is the change a reviewer most wants surfaced: this function can now reach
    // something it could not. Because effects are declared rather than inferred (v0.0.159), an
    // agent cannot add one without changing a signature — which is what makes this reportable at
    // all.
    for effect in &now.touches {
        if !was.touches.contains(effect) {
            out.push(Finding {
                verdict: WEAKENED,
                name: name.to_string(),
                detail: format!("now touches {} — it could not before", effect),
            });
        }
    }
    for effect in &was.touches {
        if !now.touches.contains(effect) {
            out.push(Finding {
                verdict: STRICTER,
                name: name.to_string(),
                detail: format!("no longer touches {}", effect),
            });
        }
    }
    if was.is_pure && !now.is_pure {
        out.push(Finding {
            verdict: WEAKENED,
            name: name.to_string(),
            detail: "no longer `pure` — its answer may now depend on more than its arguments"
                .to_string(),
        });
    }
    if !was.is_pure && now.is_pure {
        out.push(Finding {
            verdict: STRICTER,
            name: name.to_string(),
            detail: "now `pure`".to_string(),
        });
    }
    // Privacy is a promise about who may touch this. Opening it up is a weakening of exactly the
    // same kind as dropping a precondition: something that could not happen now can.
    if was.private && !now.private {
        out.push(Finding {
            verdict: WEAKENED,
            name: name.to_string(),
            detail: "no longer `private` — anything may now read it".to_string(),
        });
    }
    if !was.private && now.private {
        out.push(Finding {
            verdict: STRICTER,
            name: name.to_string(),
            detail: "now `private`".to_string(),
        });
    }
    if was.shape != now.shape {
        out.push(Finding {
            verdict: CHANGED,
            name: name.to_string(),
            detail: format!("{} -> {}", was.shape, now.shape),
        });
    }
}

fn promises_of(path: &str) -> Result<BTreeMap<String, Promise>, String> {
    // The module loader, so `use "..."` is followed exactly as a compile would follow it: a
    // review that read only one file would miss a contract deleted in another.
    let (source, _files) = crate::load_program(path)?;
    let tokens = Lexer::new(&source)
        .tokenize()
        .map_err(|d| format!("{}: {}", path, d.message))?;
    // `with_source`, NOT `new`. `Parser::new` keeps no source, so every `Contract.text` comes
    // back EMPTY — and two empty strings compare equal, so a deleted `requires` looked like no
    // change at all. The tool reported the privacy and `pure` weakenings and silently missed the
    // one it exists for. A tool that under-reports is worse than no tool, because it is believed.
    let program = Parser::with_source(tokens, &source)
        .parse()
        .map_err(|d| format!("{}: {}", path, d.message))?;
    Ok(collect(&program))
}

fn collect(prog: &Program) -> BTreeMap<String, Promise> {
    let mut out = BTreeMap::new();
    for f in &prog.fns {
        // An associated function is already stored qualified — `Account.open` — so a constructor
        // needs no special case here.
        let kind = if f.name.contains('.') { "constructor" } else { "function" };
        out.insert(
            f.name.clone(),
            Promise {
                kind,
                shape: shape_of(&f.parameters, &f.ret),
                requires: normalised(&f.requires, &f.parameters),
                ensures: normalised(&f.ensures, &f.parameters),
                is_pure: f.is_pure,
                private: false,
                touches: f.touches.clone(),
            },
        );
    }
    for m in &prog.methods {
        out.insert(
            format!("{}.{}", m.receiver, m.name),
            Promise {
                kind: "method",
                shape: shape_of(&m.parameters, &m.ret),
                requires: normalised(&m.requires, &m.parameters),
                ensures: normalised(&m.ensures, &m.parameters),
                is_pure: false,
                private: m.private,
                touches: m.touches.clone(),
            },
        );
    }
    for im in &prog.impls {
        for m in &im.methods {
            out.insert(
                format!("{}.{}", m.receiver, m.name),
                Promise {
                    kind: "method",
                    shape: shape_of(&m.parameters, &m.ret),
                    requires: normalised(&m.requires, &m.parameters),
                    ensures: normalised(&m.ensures, &m.parameters),
                    is_pure: false,
                    private: m.private,
                    touches: m.touches.clone(),
                },
            );
        }
    }
    for s in &prog.structs {
        out.insert(
            s.name.clone(),
            Promise {
                kind: "class",
                shape: format!("{} field(s)", s.fields.len()),
                requires: Vec::new(),
                ensures: Vec::new(),
                is_pure: false,
                private: false,
                touches: Vec::new(),
            },
        );
        for f in &s.fields {
            out.insert(
                format!("{}.{}", s.name, f.name),
                Promise {
                    kind: "field",
                    shape: format!("{}", f.ty),
                    requires: Vec::new(),
                    ensures: Vec::new(),
                    is_pure: false,
                    private: s.private_fields.contains(&f.name),
                    touches: Vec::new(),
                },
            );
        }
    }
    for t in &prog.interfaces {
        out.insert(
            t.name.clone(),
            Promise {
                kind: "interface",
                shape: format!("{} method(s)", t.methods.len()),
                requires: Vec::new(),
                ensures: Vec::new(),
                is_pure: false,
                private: false,
                touches: Vec::new(),
            },
        );
    }
    // A `const` is part of the surface, and its VALUE is part of it too — which is unusual
    // enough to say why.
    //
    // Everywhere else in this file the value is none of the tool's business: a function body may
    // change freely as long as the signature holds. A const has no body. It is folded at compile
    // time and SUBSTITUTED into every dependent, so `const RETRIES: Int = 3;` becoming `= 30`
    // changes what a caller's compiled code does with no signature anywhere altered — the exact
    // shape of change this tool exists to make visible. So the value goes in `shape` and a change
    // to it is reported as CHANGED, alongside a change of type.
    //
    // The initialiser is rendered as WRITTEN rather than folded, because `burxt review` reads
    // declarations only: no typechecker, so no folding. `LIMIT * 2 -> LIMIT * 3` is reported and
    // a reformat of the same expression would be too, which is the one place this is noisier than
    // the rest of the file. Named rather than hidden; the fix is a folder in the parser, and that
    // was refused for a better reason — see `Parser::parse_const`.
    for c in &prog.consts {
        out.insert(
            c.name.clone(),
            Promise {
                kind: "const",
                // No parameters, so `render`'s position-substitution does nothing here and every
                // name renders as itself — which is what a const initialiser wants: a reference
                // to another const should read as that const's name.
                shape: format!("{} = {}", c.declared, render(&c.value, &[])),
                requires: Vec::new(),
                ensures: Vec::new(),
                is_pure: false,
                private: false,
                touches: Vec::new(),
            },
        );
    }
    out
}

/// Parameter TYPES and the return type — never the parameter names. Renaming a parameter changes
/// no promise, and reporting it would be noise; noise is how a reviewer learns to skim past the
/// one line that mattered.
fn shape_of(parameters: &[Param], ret: &Type) -> String {
    let mut s = String::from("(");
    for (i, p) in parameters.iter().enumerate() {
        if i > 0 {
            s.push_str(", ");
        }
        let _ = write!(s, "{}", p.ty);
    }
    let _ = write!(s, ") -> {}", ret);
    s
}

/// A clause rendered so that RENAMING A PARAMETER does not change it.
///
/// This is the difference between a tool a reviewer reads and one they mute. Comparing
/// `Contract.text` reported `requires a <= b` as lost and `requires amount <= balance` as gained
/// when a parameter was renamed — two WEAKENED lines for a change that weakened nothing. A tool
/// that cries wolf on a rename teaches you to skim past the line that mattered.
///
/// So a parameter is rendered by its POSITION: `a <= b` becomes `#0 <= #1`, and the rename
/// vanishes. Everything else — `self`, a field, a literal, a call — renders by name, because
/// changing any of those genuinely changes the promise.
fn normalised(clauses: &[Contract], parameters: &[Param]) -> Vec<String> {
    clauses
        .iter()
        .map(|c| {
            let shape = render(&c.cond, parameters);
            // The text is kept for the MESSAGE, so a reviewer still reads what they wrote. Only
            // the comparison is normalised.
            format!("{}\u{1}{}", shape, c.text)
        })
        .collect()
}

/// Split a normalised clause back into what to compare and what to show.
fn shown(normal: &str) -> &str {
    normal.split('\u{1}').nth(1).unwrap_or(normal)
}

fn same_clause(a: &str, b: &str) -> bool {
    a.split('\u{1}').next() == b.split('\u{1}').next()
}

/// Render an expression canonically, with parameters as positions.
///
/// Unhandled kinds fall back to a discriminant plus their children rather than to nothing, so a
/// construct this does not know about still compares as ITSELF — a false positive is noise, but a
/// false negative is a weakened contract nobody was told about.
fn render(e: &Expr, parameters: &[Param]) -> String {
    match &e.kind {
        ExprKind::IntLit(n) => format!("{}", n),
        ExprKind::DecimalLit { unscaled, scale } => format!("{}e-{}", unscaled, scale),
        ExprKind::BoolLit(b) => format!("{}", b),
        ExprKind::StrLit(s) => format!("{:?}", s),
        ExprKind::Var(name) => match parameters.iter().position(|p| &p.name == name) {
            Some(i) => format!("#{}", i),
            None => name.clone(),
        },
        ExprKind::Neg(inner) => format!("-({})", render(inner, parameters)),
        ExprKind::Not(inner) => format!("!({})", render(inner, parameters)),
        ExprKind::Try(inner) => format!("({})?", render(inner, parameters)),
        ExprKind::Logical { op, lhs, rhs } => format!(
            "({} {} {})",
            render(lhs, parameters),
            op,
            render(rhs, parameters)
        ),
        ExprKind::Binary { op, lhs, rhs } => format!(
            "({} {} {})",
            render(lhs, parameters),
            op,
            render(rhs, parameters)
        ),
        ExprKind::Compare { op, lhs, rhs } => format!(
            "({} {} {})",
            render(lhs, parameters),
            op,
            render(rhs, parameters)
        ),
        ExprKind::Call { name, arguments } => {
            format!("{}({})", name, list(arguments, parameters))
        }
        ExprKind::Field { base, field } => {
            format!("{}.{}", render(base, parameters), field)
        }
        ExprKind::MethodCall { base, method, arguments } => format!(
            "{}.{}({})",
            render(base, parameters),
            method,
            list(arguments, parameters)
        ),
        ExprKind::Index { base, index } => {
            format!("{}[{}]", render(base, parameters), render(index, parameters))
        }
        ExprKind::ArrayLit(items) => format!("[{}]", list(items, parameters)),
        ExprKind::StructLit { name, fields } => {
            let mut inner: Vec<String> = fields
                .iter()
                .map(|(n, v)| format!("{}: {}", n, render(v, parameters)))
                .collect();
            inner.sort();
            format!("{} {{{}}}", name, inner.join(", "))
        }
        ExprKind::InterpStr(_) => "interp".to_string(),
    }
}

fn list(items: &[Expr], parameters: &[Param]) -> String {
    items.iter().map(|a| render(a, parameters)).collect::<Vec<_>>().join(", ")
}
