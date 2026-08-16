//! `burxt effects <file.bx>` — what can this program reach, and where did each reach enter?
//!
//! §Q1, specified since the 1.0 roadmap and unbuilt until 1.3.
//!
//! **Why this can exist at all, and why nothing else has it.** Every other language's answer to
//! "what does this program touch" is to run it and watch, or to read it and hope. Burxt makes a
//! function declare what it reaches and **refuses to compile one that under-declares** — three
//! wrappers deep, `main` calling `wrapper` calling `sneaky` calling `system` is still refused
//! until every one of them says `touches commands`. So the declarations are not documentation to
//! be trusted; they are a fact the compiler already enforced, and this command only has to read
//! and total them.
//!
//! **The consequence that makes it worth building: a caller can refuse a program before running
//! it.** A playground that accepts strangers' code can ask `--allow clock` and be told no, with
//! the path that would have reached out. Runtime sandboxing is still necessary — see the caveat
//! below — but every other playground on the internet has *only* runtime sandboxing, because
//! nothing in those languages makes a program state its reach.
//!
//! **The top level is exempt from effects** (§Q2, Andre's decision, verified v0.0.287). So this
//! cannot read an answer off `region main`; it walks the call graph from the top-level statements
//! and totals what it finds. That is the harder answer and the one §Q1 asked for.
//!
//! **Reachability is the whole difficulty.** Totalling every function in the file would be two
//! lines and useless: `use "lib/os.bx"` would report `commands` for a program that only asks the
//! time, because `os_capture` is *defined* in that module. A gate that cries wolf is a gate people
//! pass with `--allow` everything.
//!
//! **What it does not do, stated here so it is not assumed.** An effect set bounds what a program
//! *admits* to reaching. It does not bound syscalls: a submission that honestly declares
//! `touches commands` may still run `curl`. This refuses; it does not contain. Containment is a
//! container, and that is an operations problem rather than a language one.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

use crate::ast::{Effect, Expr, ExprKind, FnDef, InterpPart, Program, Stmt, StmtKind};

/// One effect, and the shortest chain of calls that reaches something declaring it.
struct Reach {
    effect: Effect,
    /// Top-level call first, the declaring `external function` or builtin last.
    path: Vec<String>,
}

pub fn report(path: &str, allow: Option<&str>, json: bool) -> Result<i32, String> {
    // Typecheck before reporting, and refuse if it fails. An effect report on a program that does
    // not compile is a lie in the most dangerous direction: the guarantee this command rests on is
    // that the checker REFUSED every under-declaration, and a program that never reached the
    // checker has no such guarantee behind it.
    let (source, _files) = crate::load_program(path)?;
    let tokens = crate::lexer::Lexer::new(&source)
        .tokenize()
        .map_err(|d| format!("{}: {}", path, d.message))?;
    let program = crate::parser::Parser::with_source(tokens, &source)
        .parse()
        .map_err(|d| format!("{}: {}", path, d.message))?;
    let mut checker = crate::typeck::TypeChecker::new();
    checker.check(&program).map_err(|ds| {
        let first = ds.first().map(|d| d.message.clone()).unwrap_or_default();
        format!("{}: {}\n(effects are only meaningful for a program that compiles)", path, first)
    })?;

    let reaches = walk(&program);

    // Parse `--allow` before printing anything, so a typo in the gate is an error rather than a
    // silent pass. `--allow ""` is meaningful and means "nothing at all".
    let allowed: Option<BTreeSet<Effect>> = match allow {
        None => None,
        Some(list) => {
            let mut set = BTreeSet::new();
            for word in list.split(',').map(|w| w.trim()).filter(|w| !w.is_empty()) {
                match Effect::parse(word) {
                    Some(e) => {
                        set.insert(e);
                    }
                    None => {
                        return Err(format!(
                            "`{}` is not an effect. The vocabulary is closed on purpose: {}",
                            word,
                            Effect::all()
                        ))
                    }
                }
            }
            Some(set)
        }
    };

    if json {
        print_json(path, &reaches, &allowed);
    } else {
        print_plain(path, &reaches, &allowed);
    }

    // Exit 70 for a gate failure — the same code every named runtime refusal uses, so a caller
    // that already treats 70 as "Burxt said no" needs no new case.
    if let Some(ok) = &allowed {
        if reaches.iter().any(|r| !ok.contains(&r.effect)) {
            return Ok(70);
        }
    }
    Ok(0)
}

/// The call graph, walked from the top level, shortest path to each effect.
fn walk(program: &Program) -> Vec<Reach> {
    // What each callable DECLARES. Because the checker enforces completeness, a declaration is
    // already the transitive answer for that callable — so the union over reachable callables is
    // the program's answer, and no fixed point is needed.
    let mut declared: HashMap<String, Vec<Effect>> = HashMap::new();
    let mut calls: HashMap<String, Vec<String>> = HashMap::new();

    for e in &program.externs {
        declared.insert(e.name.clone(), e.touches.clone());
    }
    for f in &program.fns {
        declared.insert(f.name.clone(), f.touches.clone());
        calls.insert(f.name.clone(), callees_of_fn(f));
    }
    for m in &program.methods {
        let key = format!("{}.{}", m.receiver, m.name);
        declared.insert(key.clone(), m.touches.clone());
        let mut found = Vec::new();
        for s in &m.body {
            callees_in_stmt(s, &mut found);
        }
        calls.insert(key, found);
    }

    // Builtins that carry an effect of their own. `read_file` is not an `external function` and
    // has no declaration in the source, so without this a program reaching the filesystem through
    // the builtin would report nothing at all — the worst possible failure for a gate.
    for (name, effect) in [
        ("read_file", Effect::Files),
        ("write_file", Effect::Files),
        ("write_bytes", Effect::Files),
        ("argument", Effect::Input),
        ("argument_count", Effect::Input),
    ] {
        declared.entry(name.to_string()).or_insert_with(|| vec![effect]);
    }

    // Breadth-first, so the first path found to a given effect is the shortest one. A reader
    // chasing "why does this touch the network" wants the shortest route, not the first one a
    // depth-first walk stumbled into.
    let mut seed = Vec::new();
    for s in &program.stmts {
        callees_in_stmt(s, &mut seed);
    }

    // **Two answers are collected, and the difference is the whole point of the `via` column.**
    //
    // `entered` records the path down to the thing that INTRODUCES an effect — an
    // `external function` or a builtin, something with no body of its own. That is what §Q1 asked
    // for: *where did each one enter*. `load` declaring `touches files` is not where files
    // entered; it is a wrapper that had no choice, because the checker made it declare what
    // `file_read_maybe` reaches.
    //
    // `declarer` is the fallback — the first reachable callable that declares the effect. It is
    // used only when no leaf is reachable, which should not happen and is not worth being wrong
    // about silently if it ever does.
    let mut entered: BTreeMap<Effect, Vec<String>> = BTreeMap::new();
    let mut declarer: BTreeMap<Effect, Vec<String>> = BTreeMap::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<Vec<String>> = VecDeque::new();
    for name in seed {
        queue.push_back(vec![name]);
    }

    while let Some(path) = queue.pop_front() {
        let name = path.last().expect("a path always has a last element").clone();
        if !seen.insert(name.clone()) {
            continue;
        }
        if let Some(effects) = declared.get(&name) {
            // No entry in `calls` means no body in this program: an `external function`, or a
            // builtin. That is a leaf, and a leaf is where an effect enters the language.
            let is_leaf = !calls.contains_key(&name);
            for effect in effects {
                better(&mut declarer, *effect, &path);
                if is_leaf {
                    better(&mut entered, *effect, &path);
                }
            }
        }
        if let Some(next) = calls.get(&name) {
            for callee in next {
                if !seen.contains(callee) {
                    let mut longer = path.clone();
                    longer.push(callee.clone());
                    queue.push_back(longer);
                }
            }
        }
    }

    declarer
        .into_iter()
        .map(|(effect, fallback)| Reach {
            effect,
            path: entered.remove(&effect).unwrap_or(fallback),
        })
        .collect()
}

/// Keep `path` for `effect` if nothing is kept yet, or if it beats what is.
///
/// **The tie-break is explicit, and it is what makes the two compilers agree.** Breadth-first
/// finds the shortest path, but when two leaves sit at the same depth the winner is whichever the
/// walk reached first — and stage-0 and stage-1 walk a body in orders that need not match. They
/// disagreed on exactly this: `file_read_maybe -> fopen` here, `file_read_maybe -> read_file`
/// there. Both true, both three hops, and a parity test cannot accept "either".
///
/// Shorter wins; equal length, the lexicographically smaller path wins. Neither walker has to
/// promise an order, which is the point — a rule that depends on traversal order is a rule that
/// breaks the next time either walker is edited.
fn better(best: &mut BTreeMap<Effect, Vec<String>>, effect: Effect, path: &[String]) {
    match best.get(&effect) {
        None => {
            best.insert(effect, path.to_vec());
        }
        Some(held) => {
            let closer = path.len() < held.len();
            let tied_and_smaller = path.len() == held.len() && path.join(" -> ") < held.join(" -> ");
            if closer || tied_and_smaller {
                best.insert(effect, path.to_vec());
            }
        }
    }
}

fn callees_of_fn(f: &FnDef) -> Vec<String> {
    let mut found = Vec::new();
    for s in &f.body {
        callees_in_stmt(s, &mut found);
    }
    found
}

/// Every call named anywhere in a statement.
///
/// **No `_` arm, deliberately**, here and in `callees_in_expr`. A new statement or expression kind
/// that can contain a call and is not listed here would make this command quietly under-report —
/// and an effect gate that under-reports is worse than none, because it is believed. The same
/// reasoning `typeck::allocates` gives for its own exhaustive match: a new kind should not
/// silently inherit an answer, it should stop the build until someone says which.
fn callees_in_stmt(s: &Stmt, out: &mut Vec<String>) {
    let block = |b: &Vec<Stmt>, out: &mut Vec<String>| {
        for s in b {
            callees_in_stmt(s, out);
        }
    };
    match &s.kind {
        StmtKind::Let { value, .. }
        | StmtKind::Assign { value, .. }
        | StmtKind::AssignField { value, .. }
        | StmtKind::ExprStmt(value)
        | StmtKind::Return(value)
        | StmtKind::TailReturn(value)
        | StmtKind::Print { value, .. } => callees_in_expr(value, out),
        StmtKind::AssignFieldIndex { index, value, .. }
        | StmtKind::AssignIndex { index, value, .. } => {
            callees_in_expr(index, out);
            callees_in_expr(value, out);
        }
        StmtKind::Region { body, .. } => block(body, out),
        StmtKind::For { iterable, body, .. } => {
            callees_in_expr(iterable, out);
            block(body, out);
        }
        StmtKind::ForRange { start, end, body, .. } => {
            callees_in_expr(start, out);
            callees_in_expr(end, out);
            block(body, out);
        }
        StmtKind::Match { value, arms } => {
            callees_in_expr(value, out);
            for arm in arms {
                block(&arm.body, out);
            }
        }
        StmtKind::While { cond, body } => {
            callees_in_expr(cond, out);
            block(body, out);
        }
        StmtKind::If { cond, then_block, else_block } => {
            callees_in_expr(cond, out);
            block(then_block, out);
            if let Some(b) = else_block {
                block(b, out);
            }
        }
        StmtKind::Break | StmtKind::Continue => {}
    }
}

fn callees_in_expr(e: &Expr, out: &mut Vec<String>) {
    match &e.kind {
        ExprKind::Call { name, arguments } => {
            out.push(name.clone());
            for a in arguments {
                callees_in_expr(a, out);
            }
        }
        ExprKind::MethodCall { base, method, arguments } => {
            callees_in_expr(base, out);
            // Unqualified, because the receiver's type is not known without the checker. The graph
            // carries `Type.method` keys, so this lookup misses rather than guesses — and a miss
            // costs one hop, because whatever that method reaches it had to DECLARE, and the call
            // that introduced it is inside a body this walk still visits.
            out.push(method.clone());
            for a in arguments {
                callees_in_expr(a, out);
            }
        }
        ExprKind::Binary { lhs, rhs, .. }
        | ExprKind::Logical { lhs, rhs, .. }
        | ExprKind::Compare { lhs, rhs, .. } => {
            callees_in_expr(lhs, out);
            callees_in_expr(rhs, out);
        }
        ExprKind::Neg(inner) | ExprKind::Not(inner) | ExprKind::Try(inner) => {
            callees_in_expr(inner, out)
        }
        ExprKind::Index { base, index } => {
            callees_in_expr(base, out);
            callees_in_expr(index, out);
        }
        ExprKind::Field { base, .. } => callees_in_expr(base, out),
        ExprKind::StructLit { fields, .. } => {
            for (_, v) in fields {
                callees_in_expr(v, out);
            }
        }
        ExprKind::TupleLit(items) | ExprKind::ArrayLit(items) => {
            for i in items {
                callees_in_expr(i, out);
            }
        }
        ExprKind::InterpStr(parts) => {
            for p in parts {
                match p {
                    InterpPart::Expr(inner) => callees_in_expr(inner, out),
                    InterpPart::Lit(_) => {}
                }
            }
        }
        ExprKind::IntLit(_)
        | ExprKind::DecimalLit { .. }
        | ExprKind::BoolLit(_)
        | ExprKind::StrLit(_)
        | ExprKind::Var(_) => {}
    }
}

fn print_plain(path: &str, reaches: &[Reach], allowed: &Option<BTreeSet<Effect>>) {
    if reaches.is_empty() {
        println!("{} reaches nothing outside itself.", path);
    } else {
        println!("{} can reach:", path);
        println!();
        for r in reaches {
            let refused = allowed.as_ref().is_some_and(|ok| !ok.contains(&r.effect));
            // **`{:<9}` on `r.effect` did nothing**, and the two compilers disagreed by four
            // spaces because of it. A width in a format spec is only honoured by a `Display` that
            // routes through `f.pad()`; `Effect`'s writes with `f.write_str`, so the width was
            // accepted, ignored, and `REFUSED` would have sat four columns out of line the first
            // time anyone used the gate. Formatting to a String first is what makes the spec
            // apply — the alternative is fixing `Display`, which would change every other message
            // that prints an effect.
            let name = r.effect.to_string();
            println!(
                "  {:<9} {}  via {}",
                name,
                if refused { "REFUSED" } else { "       " },
                r.path.join(" -> ")
            );
        }
    }

    if let Some(ok) = allowed {
        let over: Vec<&Reach> = reaches.iter().filter(|r| !ok.contains(&r.effect)).collect();
        println!();
        if over.is_empty() {
            let names: Vec<String> = ok.iter().map(|e| e.to_string()).collect();
            println!(
                "allowed: {}. Nothing outside it is reachable.",
                if names.is_empty() { "nothing".to_string() } else { names.join(", ") }
            );
        } else {
            let names: Vec<String> = over.iter().map(|r| r.effect.to_string()).collect();
            println!("REFUSED: {} is outside what was allowed.", names.join(", "));
            println!("The compiler enforced these declarations, so this is what the program CAN");
            println!("reach, not what it happens to do on one run.");
        }
    }
}

fn print_json(path: &str, reaches: &[Reach], allowed: &Option<BTreeSet<Effect>>) {
    let escape = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    println!("{{");
    println!("  \"file\": \"{}\",", escape(path));
    print!("  \"effects\": [");
    for (i, r) in reaches.iter().enumerate() {
        if i > 0 {
            print!(",");
        }
        print!("\n    {{ \"effect\": \"{}\", \"via\": [", r.effect);
        for (j, step) in r.path.iter().enumerate() {
            if j > 0 {
                print!(", ");
            }
            print!("\"{}\"", escape(step));
        }
        print!("] }}");
    }
    println!("{}],", if reaches.is_empty() { "" } else { "\n  " });
    let refused: Vec<&Reach> = match allowed {
        None => Vec::new(),
        Some(ok) => reaches.iter().filter(|r| !ok.contains(&r.effect)).collect(),
    };
    print!("  \"refused\": [");
    for (i, r) in refused.iter().enumerate() {
        if i > 0 {
            print!(", ");
        }
        print!("\"{}\"", r.effect);
    }
    println!("]");
    println!("}}");
}
