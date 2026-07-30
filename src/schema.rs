//! `burxt mcp-schema <file.bx>` — the MCP tool manifest, derived from the preconditions.
//!
//! This is the one thing in this repository that no other language can do, and the reason is not
//! cleverness. It is that a precondition lives in the **signature**:
//!
//! ```text
//! function line_total(unit: Decimal<2> [> $0.00], quantity: Int [> 0, <= 100000]) -> Decimal<2>
//! ```
//!
//! Everywhere else the JSON Schema a client validates against and the check the function performs are
//! **two artifacts maintained by hand**, and the schema is the one that rots. It says a field is
//! optional after the code started requiring it, or keeps a bound the code relaxed a year ago. The
//! client sends a request that is valid by the schema, the tool refuses it, and the failure arrives as
//! a 500 rather than as a validation message — which is the worst possible place to learn about it,
//! because the schema was the thing that was supposed to prevent exactly that.
//!
//! Here there is one place to change. Forgetting to change the other is not a thing that can happen,
//! because there is no other.
//!
//! ## What it reads, and what it deliberately does not
//!
//! Clauses are read STRUCTURALLY, from the parsed condition, not from the text — so `[> $0.00]` and a
//! written `requires unit > $0.00` produce identical schema, which is what makes them the same
//! sentence rather than two spellings that happen to agree today.
//!
//! A clause it cannot express is **skipped and reported**, never guessed at. `requires a <= b` relates
//! two parameters and JSON Schema has no way to say that; emitting nothing for it is honest, and
//! emitting something approximate would be the drift this tool exists to remove. The count of skipped
//! clauses goes to stderr so a schema that covers less than the function does says so out loud.
//!
//! Money is `"type": "string"` throughout, for the reason lib/json.bx's header gives: a JSON number
//! reaches a JavaScript consumer as a double and loses the cent.

use crate::ast::{CmpOp, Expr, ExprKind, FnDef, Program, Type};
use crate::lexer::Lexer;
use crate::parser::Parser;

/// A bound this tool was able to express, as JSON Schema's key and value.
struct Bound {
    key: &'static str,
    value: String,
}

/// The digits of a numeric literal, exactly — never through a float.
///
/// A `DecimalLit` is already an unscaled integer and a scale, which is precisely what a decimal
/// string is, so rendering it is inserting a point rather than converting anything. That is the same
/// reason `lib/json.bx` keeps a JSON number as its digits.
fn literal_digits(e: &Expr) -> Option<String> {
    match &e.kind {
        ExprKind::IntLit(n) => Some(n.to_string()),
        ExprKind::DecimalLit { unscaled, scale } => {
            let negative = *unscaled < 0;
            let magnitude = unscaled.unsigned_abs().to_string();
            let s = *scale as usize;
            let padded = if magnitude.len() <= s {
                format!("{}{}", "0".repeat(s + 1 - magnitude.len()), magnitude)
            } else {
                magnitude
            };
            let at = padded.len() - s;
            let text = if s == 0 {
                padded
            } else {
                format!("{}.{}", &padded[..at], &padded[at..])
            };
            Some(if negative { format!("-{}", text) } else { text })
        }
        // A unary minus in front of a literal, which is how `-$100.00` parses.
        ExprKind::Neg(inner) => literal_digits(inner).map(|d| {
            if let Some(stripped) = d.strip_prefix('-') {
                stripped.to_string()
            } else {
                format!("-{}", d)
            }
        }),
        _ => None,
    }
}

/// The bound a clause states about `param`, if it states one this tool can express.
///
/// `None` covers three different situations on purpose, and none of them is an error: the clause is
/// about a different parameter, it relates two parameters (`amount <= balance`), or it is a shape JSON
/// Schema has no key for. All three are reported as skipped rather than approximated.
fn bound_for(param: &str, cond: &Expr) -> Option<Bound> {
    let ExprKind::Compare { op, lhs, rhs } = &cond.kind else {
        return None;
    };
    // `param OP literal`, the elided bracket form's shape and the one people write by hand.
    let ExprKind::Var(named) = &lhs.kind else {
        return None;
    };
    if named != param {
        return None;
    }
    let value = literal_digits(rhs)?;
    let key = match op {
        CmpOp::Gt => "exclusiveMinimum",
        CmpOp::Ge => "minimum",
        CmpOp::Lt => "exclusiveMaximum",
        CmpOp::Le => "maximum",
        // Equality is a constant, not a bound. `const` is the JSON Schema key for it and it is a
        // strange thing to write as a precondition, so it is skipped and counted rather than guessed.
        CmpOp::Eq | CmpOp::Ne => return None,
    };
    Some(Bound { key, value })
}

/// What a Burxt type is on the wire.
///
/// Money and every other decimal is a **string**, which is the position lib/json.bx takes: a JSON
/// number reaches a JavaScript consumer as a double, and `19.99` stops being `19.99`. An `Int` is a
/// number because it survives one.
fn wire_type(ty: &Type) -> &'static str {
    match ty {
        Type::Int => "integer",
        Type::Bool => "boolean",
        Type::Decimal { .. } => "string",
        _ => "string",
    }
}

fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// One tool entry, or `None` when the function is not exposable.
fn tool_of(f: &FnDef, skipped: &mut usize) -> Option<String> {
    // A generic function has no concrete schema until a caller says what `T` is, and an MCP client
    // has no way to say. Reported rather than emitted half-formed.
    if !f.type_parameters.is_empty() {
        return None;
    }
    let mut properties: Vec<String> = Vec::new();
    let mut required: Vec<String> = Vec::new();
    for p in &f.parameters {
        let mut bounds: Vec<Bound> = Vec::new();
        for c in &f.requires {
            if let Some(b) = bound_for(&p.name, &c.cond) {
                bounds.push(b);
            }
        }
        let mut entry = format!("{{\"type\":{}", quote(wire_type(&p.ty)));
        // The declared type, verbatim, as the description. A client's model reads this, and
        // `Decimal<2>` says more about what is wanted than any sentence would — it says the scale.
        entry.push_str(&format!(",\"description\":{}", quote(&format!("{}", p.ty))));
        for b in &bounds {
            entry.push_str(&format!(",{}:{}", quote(b.key), quote(&b.value)));
        }
        entry.push('}');
        properties.push(format!("{}:{}", quote(&p.name), entry));
        // Every parameter is required, because Burxt has no optional ones and no defaults. A schema
        // that said otherwise would be describing a different language.
        required.push(quote(&p.name));
    }
    // Clauses that named no parameter, or related two, or had no key — counted so the caller can see
    // that the schema covers less than the function checks.
    for c in &f.requires {
        if !f.parameters.iter().any(|p| bound_for(&p.name, &c.cond).is_some()) {
            *skipped += 1;
        }
    }
    let description = format!("{} -> {}", f.name, f.ret);
    Some(format!(
        "{{\"name\":{},\"description\":{},\"inputSchema\":{{\"type\":\"object\",\"properties\":{{{}}},\"required\":[{}]}}}}",
        quote(&f.name),
        quote(&description),
        properties.join(","),
        required.join(",")
    ))
}

/// The manifest, and how many clauses could not be expressed.
///
/// `own` is the byte range of the file that was ASKED for. Only functions declared inside it become
/// tools — everything reached through `use` is a dependency, not an interface.
///
/// That filter is not tidiness. The loader concatenates, so without it `burxt mcp-schema` on a
/// three-line server published the entire standard library: `string_find`, `file_delete`,
/// `os_run`. A manifest is a list of things a client is invited to call, and inviting a model to
/// call `os_run` because it happened to be in scope is the failure mode this whole project is
/// against — a plausible-looking artifact that grants far more than anyone intended.
pub fn manifest(prog: &Program, own: (usize, usize)) -> (String, usize) {
    let mut skipped = 0;
    let mut tools: Vec<String> = Vec::new();
    // `prog.externs` is a separate list, so a C declaration cannot reach here and needs no filter.
    for f in &prog.fns {
        let at = f.span.start as usize;
        if at < own.0 || at >= own.0 + own.1 {
            continue;
        }
        if let Some(t) = tool_of(f, &mut skipped) {
            tools.push(t);
        }
    }
    (format!("{{\"tools\":[{}]}}", tools.join(",")), skipped)
}

/// `burxt mcp-schema <file.bx>` — print the manifest, and say what it could not express.
pub fn emit(path: &str) -> Result<i32, String> {
    // The module loader, so `use "..."` is followed exactly as a compile would follow it. A schema
    // read from one file would miss a tool declared in another, and silently.
    let (source, files) = crate::load_program(path)?;
    let tokens = Lexer::new(&source)
        .tokenize()
        .map_err(|d| format!("{}: {}", path, d.message))?;
    // `with_source`, for the reason `review` records: `Parser::new` keeps no source, so contract text
    // comes back empty. This tool reads conditions structurally rather than as text, so it would have
    // survived that — but the two loaders staying identical is worth more than the saving.
    let program = Parser::with_source(tokens, &source)
        .parse()
        .map_err(|d| format!("{}: {}", path, d.message))?;
    // The file that was asked for, by its absolute path as the loader recorded it. Only its own
    // declarations are tools; see `manifest`.
    let wanted = std::fs::canonicalize(path)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| path.to_string());
    let own = files
        .iter()
        .find(|f| {
            std::fs::canonicalize(&f.path)
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|_| f.path.clone())
                == wanted
        })
        .map(|f| (f.start, f.len))
        .ok_or_else(|| format!("{}: the loader did not record this file", path))?;
    let (manifest, skipped) = manifest(&program, own);
    println!("{}", manifest);
    if skipped > 0 {
        // To STDERR, so the manifest on stdout stays pipeable — and said out loud, because a schema
        // that covers less than the function checks is exactly the drift this tool exists to prevent.
        // Silence here would make an incomplete schema look like a complete one.
        eprintln!(
            "note: {} precondition(s) could not be expressed as JSON Schema and were left out. \
             A clause relating two parameters — `requires amount <= balance` — has no key in JSON \
             Schema, and the function still enforces it.",
            skipped
        );
    }
    Ok(0)
}
