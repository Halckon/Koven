//! Synthetic lengths exercise production guards without allocating multi-GB buffers.
use super::{TestDirectory, UnitSource};
use lang_frontend::name_resolution::SymbolKind;
use std::{fs, os::unix::process::ExitStatusExt, process::Command};

fn run_probe(llvm: &str, c: &str, argument: &str) -> std::process::Output {
    let directory = TestDirectory::create();
    let ir = directory.join("probe.ll");
    let shim = directory.join("probe.c");
    let executable = directory.join("probe");
    fs::write(&ir, llvm).unwrap();
    fs::write(&shim, c).unwrap();
    let linked = Command::new(crate::test_support::ir_clang())
        .args(["-Wno-override-module", "-fno-builtin"])
        .arg(&ir)
        .arg(&shim)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    Command::new(&executable)
        .arg(argument)
        .output()
        .expect("probe must launch")
}

#[test]
fn string_concat_native_length_guard_precedes_allocation_and_reads() {
    let analysis = UnitSource::analyze(
        "length.ko",
        "fun combine(left: String, right: String): Unit { println(left + right) }",
    )
    .expect("source must pass Unit admission");
    let (program, entry) = analysis
        .lower_entry(analysis.declaration("combine", SymbolKind::Function))
        .expect("borrowed String function lowers");
    let llvm = crate::llvm::render_verified_program(&program).unwrap();
    let function = program
        .module(entry.module())
        .unwrap()
        .function(entry)
        .unwrap();
    // Unit names include declaration/instance suffixes; use the actual lowered identity.
    let symbol = format!("f{}.{}", entry.index(), function.name);
    assert_eq!(
        llvm.matches(&format!("define internal void @{symbol}("))
            .count(),
        1,
        "the selected SSA entry must identify exactly one LLVM function"
    );
    // Export only the test entry; leave its production body and checks untouched.
    let llvm = llvm
        .replace(&format!("@{symbol}("), "@test_concat(")
        .replace(
            "define internal void @test_concat(",
            "define void @test_concat(",
        )
        .replace("@malloc(", "@probe_malloc(");
    let shim = r#"
#include <stdint.h>
#include <stdlib.h>
struct text { const void *bytes; size_t length, capacity; };
extern void test_concat(const struct text *, const struct text *);
void *probe_malloc(size_t size) { (void)size; _Exit(42); }
int main(int argc, char **argv) {
    (void)argc;
    struct text left = {0, 2147483647, 0}, right = {0, 0, 0};
    if (argv[1][0] == 'o') right.length = 1;
    if (argv[1][0] == 'w') { left.length = SIZE_MAX; right.length = 1; }
    test_concat(&left, &right);
    return 90;
}
"#;
    // Exactly the limit reaches allocation. The shim exits before dereferencing fake bytes.
    let accepted = run_probe(&llvm, shim, "boundary");
    assert_eq!(accepted.status.code(), Some(42), "{accepted:?}");
    for case in ["oversize", "wrap"] {
        let rejected = run_probe(&llvm, shim, case);
        assert_eq!(rejected.status.signal(), Some(6), "{case}: {rejected:?}");
        assert!(
            rejected.stderr.starts_with(b"abort: size overflow\n"),
            "{rejected:?}"
        );
    }
}

#[test]
fn argv_native_oversize_returns_before_utf8_allocation_and_main() {
    use crate::{llvm::entry::NativeEntryPlan, ssa::model::EntityType};
    let analysis = UnitSource::analyze(
        "argv-length.ko",
        "fun main(args: Array<String>): Unit { println(\"entered\") }",
    )
    .expect("argv source admission");
    let (program, function) = analysis
        .lower_entry(analysis.declaration("main", SymbolKind::Function))
        .expect("argv lowering");
    let module = program.module(function.module()).unwrap();
    let body = module.function(function).unwrap();
    let EntityType::Loan {
        target: arguments, ..
    } = body.entity(body.blocks[0].parameters[0]).unwrap().ty
    else {
        panic!("argv shared loan");
    };
    let (_, string) = module.sequential_container(arguments).unwrap();
    let llvm = crate::llvm::render_verified_program_with_entry_plan(
        &program,
        NativeEntryPlan::BorrowedArguments {
            function,
            arguments,
            string,
        },
    )
    .unwrap()
    .replace("@strlen(", "@probe_strlen(")
    .replace("@malloc(", "@probe_malloc(")
    .replace("@main(", "@probe_entry(");
    let shim = r#"
#include <stdint.h>
#include <stdlib.h>
extern int probe_entry(int, char **);
size_t probe_strlen(const char *s) { (void)s; return (size_t)2147483648; }
void *probe_malloc(size_t size) { (void)size; _Exit(42); }
int main(void) {
    /* Invalid readable memory: length rejection must precede UTF-8 reads. */
    char *arguments[] = {0, (char *)(uintptr_t)1};
    return probe_entry(2, arguments);
}
"#;
    let rejected = run_probe(&llvm, shim, "oversize");
    assert_eq!(rejected.status.code(), Some(1), "{rejected:?}");
    assert!(
        rejected.stdout.is_empty() && rejected.stderr.is_empty(),
        "{rejected:?}"
    );
}

#[test]
fn standard_checked_plus_returns_null_only_for_length_overflow_and_aborts_on_oom() {
    // Keep the exact production algorithm: this fixture selects its source, never rewrites it.
    let standard = include_str!("../../lang-std/koven/text/text.ko");
    let (_, body) = standard
        .split_once("borrow fun String.checkedPlus")
        .expect("production checkedPlus declaration");
    let source = format!(
        "package koven.text\nborrow fun String.checkedPlus{body}\nfun probe(left: String, right: String): Unit {{ val joined = left.checkedPlus(right); if (joined == null) {{ println(\"overflow\") }} else {{ println(\"within\") }} }}"
    );
    let analysis = UnitSource::analyze_with_type_environment(
        "checked-plus.ko",
        &source,
        |sources, source, environment| {
            environment.authorize_text_source(sources, source).unwrap();
        },
    )
    .expect("the selected production source must pass admission");
    let (program, entry) = analysis
        .lower_entry(analysis.declaration("probe", SymbolKind::Function))
        .unwrap();
    let function = program
        .module(entry.module())
        .unwrap()
        .function(entry)
        .unwrap();
    let symbol = format!("f{}.{}", entry.index(), function.name);
    let llvm = crate::llvm::render_verified_program(&program).unwrap();
    assert_eq!(
        llvm.matches(&format!("define internal void @{symbol}("))
            .count(),
        1
    );
    let llvm = llvm
        .replace(&format!("@{symbol}("), "@probe_checked_plus(")
        .replace(
            "define internal void @probe_checked_plus(",
            "define void @probe_checked_plus(",
        )
        .replace("@malloc(", "@probe_malloc(");
    let shim = r#"
#include <stdint.h>
#include <stdlib.h>
struct text { const void *bytes; size_t length, capacity; };
extern void probe_checked_plus(const struct text *, const struct text *);
static int fail_allocation;
void *probe_malloc(size_t size) {
    (void)size;
    if (fail_allocation) return 0;
    _Exit(42);
}
int main(int argc, char **argv) {
    (void)argc;
    /* Unreadable data makes any premature byte access observable. */
    struct text left = {(const void *)(uintptr_t)1, 2147483647, 0};
    struct text right = {(const void *)(uintptr_t)1, 0, 0};
    if (argv[1][0] == 'o') right.length = 1;
    if (argv[1][0] == 'f') { left.length = 1; fail_allocation = 1; }
    probe_checked_plus(&left, &right);
    return 0;
}
"#;
    let accepted = run_probe(&llvm, shim, "boundary");
    assert_eq!(accepted.status.code(), Some(42), "{accepted:?}");
    assert!(
        accepted.stdout.is_empty() && accepted.stderr.is_empty(),
        "{accepted:?}"
    );
    let overflow = run_probe(&llvm, shim, "overflow");
    assert_eq!(overflow.status.code(), Some(0), "{overflow:?}");
    assert_eq!(overflow.stdout, b"overflow\n");
    assert!(overflow.stderr.is_empty(), "{overflow:?}");
    let oom = run_probe(&llvm, shim, "failure");
    assert_eq!(oom.status.signal(), Some(6), "{oom:?}");
    assert!(
        oom.stderr.starts_with(b"abort: allocation failed\n"),
        "{oom:?}"
    );
    assert!(oom.stdout.is_empty(), "{oom:?}");
}
