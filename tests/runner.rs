//! Integration tests: lock in the observable behavior of the burxt compiler.
//!
//! Data-driven layout:
//!   tests/pass/NAME.bx  + NAME.stdout  — must compile & run; stdout must match exactly.
//!   tests/fail/NAME.bx  + NAME.stderr  — must be rejected; stderr must contain the text.
//!   tests/panic/NAME.bx + NAME.stderr  — must compile, but die at runtime with
//!                                        a nonzero exit and that text on stderr.
//!
//! Each program is compiled with the real `burxt` binary (CARGO_BIN_EXE_burxt)
//! inside a scratch directory, so executables and object files never land in
//! the repository. Adding a test = dropping two files in the right directory.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Collect (program, expected-text) pairs from tests/<dir>, where the expected
/// text lives in a sibling file with the given extension.
fn cases(dir: &str, expected_ext: &str) -> Vec<(PathBuf, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join(dir);
    let mut out = Vec::new();
    for entry in fs::read_dir(&root).unwrap_or_else(|e| panic!("cannot read {}: {}", root.display(), e)) {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) == Some("bx") {
            let expected_path = path.with_extension(expected_ext);
            let expected = fs::read_to_string(&expected_path)
                .unwrap_or_else(|e| panic!("missing expectation file {}: {}", expected_path.display(), e));
            out.push((path, expected));
        }
    }
    out.sort();
    assert!(!out.is_empty(), "no .bx programs found in tests/{}", dir);
    out
}

/// Run `burxt <cmd> <program>` in a scratch working directory.
fn burxt(cmd: &str, program: &Path, workdir: &Path) -> Output {
    fs::create_dir_all(workdir).unwrap();
    Command::new(env!("CARGO_BIN_EXE_burxt"))
        .arg(cmd)
        .arg(program)
        .current_dir(workdir)
        .output()
        .expect("failed to spawn burxt")
}

fn scratch_dir(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("burxt-tests-{}-{}", std::process::id(), tag))
}

#[test]
fn pass_programs_produce_expected_stdout() {
    let scratch = scratch_dir("pass");
    let mut failures = Vec::new();
    for (program, expected) in cases("pass", "stdout") {
        let out = burxt("run", &program, &scratch);
        let stdout = String::from_utf8_lossy(&out.stdout);
        if !out.status.success() {
            failures.push(format!(
                "{}: expected success, but compilation/run failed:\n{}",
                program.display(),
                String::from_utf8_lossy(&out.stderr)
            ));
        } else if stdout != expected {
            failures.push(format!(
                "{}: stdout mismatch\n  expected: {:?}\n  actual:   {:?}",
                program.display(),
                expected,
                stdout
            ));
        }
    }
    let _ = fs::remove_dir_all(&scratch);
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn panic_programs_die_cleanly_at_runtime() {
    let scratch = scratch_dir("panic");
    let mut failures = Vec::new();
    for (program, expected) in cases("panic", "stderr") {
        let needle = expected.trim();
        let out = burxt("run", &program, &scratch);
        let stderr = String::from_utf8_lossy(&out.stderr);
        if out.status.success() {
            failures.push(format!(
                "{}: expected a runtime error, but it ran successfully",
                program.display()
            ));
        } else if !stderr.contains(needle) {
            failures.push(format!(
                "{}: wrong runtime error\n  expected to contain: {:?}\n  actual stderr:       {:?}",
                program.display(),
                needle,
                stderr
            ));
        }
    }
    let _ = fs::remove_dir_all(&scratch);
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// The forward guarantee the object model depends on: an aggregate's layout is
/// EXACTLY its declared fields, in order, standard alignment — no type tag, no
/// vtable pointer, no refcount, no hidden header word. If this ever fails,
/// adding a trait implementation could move a field, and codegen written
/// against these offsets would break.
#[test]
fn struct_layout_has_no_hidden_header() {
    let scratch = scratch_dir("layout");
    fs::create_dir_all(&scratch).unwrap();
    let program = scratch.join("layout_probe.bx");
    fs::write(
        &program,
        "struct Money { amount: Decimal<2> }\n\
         struct LineItem { price: Decimal<2>, qty: Int }\n\
         struct Order { total: Money, items: Int, label: String }\n\
         print(1);\n",
    )
    .unwrap();

    let out = burxt("layout", &program, &scratch);
    let report = String::from_utf8_lossy(&out.stdout);
    let expected = "\
Money: size 8 align 8
  +0 Decimal<2> (8 bytes)
LineItem: size 16 align 8
  +0 Decimal<2> (8 bytes)
  +8 Int (8 bytes)
Order: size 24 align 8
  +0 Money (8 bytes)
  +8 Int (8 bytes)
  +16 String (8 bytes)
";
    let _ = fs::remove_dir_all(&scratch);
    assert!(out.status.success(), "layout command failed: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(
        report, expected,
        "layout drifted — a hidden header or reordering would break the object model"
    );
}

/// The A4.5 layout guarantee, cashed in by A4.6: a struct's field offsets must
/// be byte-identical whether or not it is ever used as a trait object, because
/// the vtable lives OUTSIDE the value. Also checks the pay-for-what-you-use
/// rule: a program with no `dyn` emits no vtable at all.
#[test]
fn dyn_does_not_change_layout_and_costs_nothing_unused() {
    let scratch = scratch_dir("dyn-layout");
    fs::create_dir_all(&scratch).unwrap();

    let common = "trait Priced { fn price(self) -> Decimal<2> }\n\
                  struct Book { cost: Decimal<2>, pages: Int }\n\
                  impl Priced for Book {\n\
                  fn (self: Book) price() -> Decimal<2> { return self.cost; }\n\
                  }\n\
                  let b: Book = Book { cost: 1.00, pages: 2 };\n";

    let static_only = scratch.join("static_only.bx");
    fs::write(&static_only, format!("{}print(b.price());\n", common)).unwrap();

    let with_dyn = scratch.join("with_dyn.bx");
    fs::write(
        &with_dyn,
        format!("{}let d: dyn Priced = b;\nprint(d.price());\n", common),
    )
    .unwrap();

    let layout_static = burxt("layout", &static_only, &scratch);
    let layout_dyn = burxt("layout", &with_dyn, &scratch);
    let ir_static = burxt("emit-ir", &static_only, &scratch);
    let ir_dyn = burxt("emit-ir", &with_dyn, &scratch);

    let l_static = String::from_utf8_lossy(&layout_static.stdout).to_string();
    let l_dyn = String::from_utf8_lossy(&layout_dyn.stdout).to_string();
    let s_ir = String::from_utf8_lossy(&ir_static.stdout).to_string();
    let d_ir = String::from_utf8_lossy(&ir_dyn.stdout).to_string();
    let _ = fs::remove_dir_all(&scratch);

    assert!(
        l_static.contains("+0 Decimal<2>") && l_static.contains("+8 Int"),
        "unexpected baseline layout:\n{}",
        l_static
    );
    assert_eq!(
        l_static, l_dyn,
        "becoming a trait object moved a field — the vtable must live outside the value"
    );
    assert!(
        !s_ir.contains("bx.vtable"),
        "a program with no `dyn` must emit no vtable"
    );
    assert!(
        d_ir.contains("bx.vtable.Priced.Book"),
        "a `dyn` program must emit the (Type, Trait) vtable"
    );
}

#[test]
fn fail_programs_are_rejected_with_expected_error() {
    let scratch = scratch_dir("fail");
    let mut failures = Vec::new();
    for (program, expected) in cases("fail", "stderr") {
        let needle = expected.trim();
        let out = burxt("build", &program, &scratch);
        let stderr = String::from_utf8_lossy(&out.stderr);
        if out.status.success() {
            failures.push(format!(
                "{}: expected rejection, but it compiled successfully",
                program.display()
            ));
        } else if !stderr.contains(needle) {
            failures.push(format!(
                "{}: wrong error\n  expected to contain: {:?}\n  actual stderr:       {:?}",
                program.display(),
                needle,
                stderr
            ));
        }
    }
    let _ = fs::remove_dir_all(&scratch);
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}
