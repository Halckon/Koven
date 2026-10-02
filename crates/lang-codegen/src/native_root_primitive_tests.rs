//! SPEC-0244：源码原语真实 link/run 与精确 owner 释放。
use super::{TestDirectory, analyze, emit_link_and_run, symbol};
use lang_frontend::name_resolution::SymbolKind;
use std::{fs, process::Command};

#[test]
fn root_primitive_native_string_exchange_and_source_order() {
    let output = emit_link_and_run(
        "root-exchange.ko",
        r#"
        fun make(): String { println("evaluate-new") return "new" + "value" }
        fun entry(): Unit {
            var a = "old" + "value"
            var b = "second" + "value"
            val old = replace(&a, make())
            println(old)
            println(a)
            swap(&a, &b)
            println(a)
            println(b)
            println(replace(&b, "last"))
            println(b)
        }
    "#,
        "entry",
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        output.stdout,
        b"evaluate-new\noldvalue\nnewvalue\nsecondvalue\nnewvalue\nnewvalue\nlast\n"
    );
}

#[test]
fn root_primitive_native_copyable_snapshots_and_repeated_swap() {
    let output = emit_link_and_run(
        "root-copy.ko",
        r#"
        fun entry(): Unit {
            var a = 1
            var b = a
            val snapshot = a
            val old = replace(&a, b)
            { b = 2 }
            swap(&a, &b)
            if (a == 2 && b == 1 && old == 1 && snapshot == 1) { println("copied") }
            swap(&a, &b)
            if (a == 1 && b == 2) { println("swapped") }
        }
    "#,
        "entry",
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(output.stdout, b"copied\nswapped\n");
}

#[test]
fn root_primitive_native_control_exit_retains_initialized_target() {
    let output = emit_link_and_run(
        "root-exit.ko",
        r#"
        fun breakCase(flag: Boolean): Unit {
            var target = "old" + "break"
            loop {
                val old = replace(&target, if (flag) break else "new")
                println(old)
                break
            }
            println(target)
        }
        fun returnCase(flag: Boolean): Unit {
            var target = "old" + "return"
            val old = replace(&target, if (flag) return else "new")
            println(old)
            println(target)
        }
        fun entry(): Unit {
            breakCase(true)
            breakCase(false)
            returnCase(true)
            returnCase(false)
        }
    "#,
        "entry",
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(output.stdout, b"oldbreak\noldbreak\nnew\noldreturn\nnew\n");
}

#[test]
fn root_primitive_native_all_storage_owners_free_exactly_once() {
    let source = r#"
        class Node(val value: String)
        value class Inline(val value: String)
        fun entry(): Unit {
            var a = "old" + "string"
            var b = "other" + "string"
            val old = replace(&a, "new" + "string")
            swap(&a, &b)
            println(old)
            println(a)
            println(b)
            var node = Node("payload" + "one")
            var otherNode = Node("payload" + "two")
            val oldNode = replace(&node, Node("payload" + "three"))
            swap(&node, &otherNode)
            var box = Box(Inline("box" + "one"))
            var otherBox = Box(Inline("box" + "two"))
            val oldBox = replace(&box, Box(Inline("box" + "three")))
            swap(&box, &otherBox)
            var rc = Rc("rc" + "one")
            var otherRc = Rc("rc" + "two")
            val oldRc = replace(&rc, Rc("rc" + "three"))
            swap(&rc, &otherRc)
            var list = listOf("list" + "one")
            var otherList = listOf("list" + "two")
            val oldList = replace(&list, listOf("list" + "three"))
            swap(&list, &otherList)
            var inline = Inline("inline" + "one")
            var otherInline = Inline("inline" + "two")
            val oldInline = replace(&inline, Inline("inline" + "three"))
            swap(&inline, &otherInline)
        }
    "#;
    let analysis = analyze("root-counted.ko", source);
    assert!(
        analysis.typed.diagnostics().is_empty(),
        "{:?}",
        analysis.typed.diagnostics()
    );
    assert!(
        analysis.owned.diagnostics().is_empty(),
        "{:?}",
        analysis.owned.diagnostics()
    );
    let (program, entry) = crate::ssa::lower_scalar_file_with_entry(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
        symbol(&analysis, "entry", SymbolKind::Function),
    )
    .expect("source root owners lower to verified SSA");
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry)
        .unwrap()
        .replace("@malloc(", "@counted_malloc(")
        .replace("@free(", "@counted_free(");
    // Six families each own three concat buffers; class, Box, Rc and List each
    // add three heap headers/payload allocations. Exchange itself allocates nothing.
    run_counted(&llvm, 30, b"oldstring\notherstring\nnewstring\n");
}

fn run_counted(llvm: &str, count: usize, stdout: &[u8]) {
    let directory = TestDirectory::create();
    let ir = directory.join("root.ll");
    let counter = directory.join("counter.c");
    let executable = directory.join("root");
    fs::write(&ir, llvm).unwrap();
    fs::write(
        &counter,
        format!(
            r#"
#include <stdlib.h>
#include <assert.h>
static void *live[{count}];
static int allocations, releases;
void *counted_malloc(size_t size) {{
    assert(size > 0 && allocations < {count});
    void *pointer = malloc(size);
    assert(pointer);
    for (int i = 0; i < allocations; ++i) assert(live[i] != pointer);
    live[allocations++] = pointer;
    return pointer;
}}
void counted_free(void *pointer) {{
    assert(pointer);
    for (int i = 0; i < allocations; ++i) {{
        if (live[i] == pointer) {{ live[i] = 0; ++releases; free(pointer); return; }}
    }}
    abort();
}}
__attribute__((destructor)) static void verify_counts(void) {{
    assert(allocations == {count} && releases == {count});
    for (int i = 0; i < allocations; ++i) assert(!live[i]);
}}
"#
        ),
    )
    .unwrap();
    let link = Command::new(crate::test_support::ir_clang())
        .arg(&ir)
        .arg(&counter)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(link.status.success(), "{link:?}");
    let run = Command::new(&executable).output().unwrap();
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, stdout);
}

#[test]
fn root_primitive_native_conditional_commit_and_return_free_each_owner_once() {
    let analysis = analyze(
        "root-conditional-counts.ko",
        r#"
        fun test(flag: Boolean): Unit {
            var target = "old" + "value"
            val old = replace(&target, if (flag) return else "new" + "value")
            println(old)
            println(target)
        }
        fun entry(): Unit { test(true)
 test(false) }
    "#,
    );
    let (program, entry) = crate::ssa::lower_scalar_file_with_entry(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
        symbol(&analysis, "entry", SymbolKind::Function),
    )
    .expect("conditional return keeps old root until normal commit");
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry)
        .unwrap()
        .replace("@malloc(", "@counted_malloc(")
        .replace("@free(", "@counted_free(");
    run_counted(&llvm, 3, b"oldvalue\nnewvalue\n");
}

#[test]
fn root_primitive_native_abort_never_commits_or_drops_the_old_root() {
    let analysis = analyze(
        "root-abort-counts.ko",
        r#"
        fun entry(): Unit {
            var target = "old" + "value"
            val old = replace(&target, error("stop"))
        }
    "#,
    );
    let (program, entry) = crate::ssa::lower_scalar_file_with_entry(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
        symbol(&analysis, "entry", SymbolKind::Function),
    )
    .expect("abort has a prefix, no commit");
    assert!(!crate::ssa::render_program(&program).contains("root_replace"));
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry)
        .unwrap()
        .replace("@malloc(", "@counted_malloc(")
        .replace("@free(", "@unexpected_free(")
        .replace("@abort(", "@expected_abort(");
    let directory = TestDirectory::create();
    let ir = directory.join("abort.ll");
    let counter = directory.join("counter.c");
    let executable = directory.join("abort");
    fs::write(&ir, llvm).unwrap();
    fs::write(&counter, r#"
#include <stdlib.h>
#include <assert.h>
static void *owner;
void *counted_malloc(size_t size) { assert(!owner && size == 8); owner = malloc(size); assert(owner); return owner; }
void unexpected_free(void *pointer) { (void)pointer; _Exit(41); }
void expected_abort(void) { assert(owner); _Exit(0); }
__attribute__((destructor)) static void unexpected_return(void) { _Exit(42); }
"#).unwrap();
    let link = Command::new(crate::test_support::ir_clang())
        .arg(&ir)
        .arg(&counter)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(link.status.success(), "{link:?}");
    let run = Command::new(&executable).output().unwrap();
    assert!(run.status.success(), "{run:?}");
    assert!(run.stdout.is_empty() && run.stderr.is_empty(), "{run:?}");
}

#[test]
fn root_primitive_native_unit_loop_exit_keeps_initialized_storage() {
    let run = emit_link_and_run(
        "unit-root-loop.ko",
        r#"
        fun unit(): Unit {}
        fun test(flag: Boolean): Unit {
            var a = unit()
            loop {
                val old = replace(&a, if (flag) break else unit())
                break
            }
            var b = unit()
            swap(&a, &b)
            println("unit-ready")
        }
        fun entry(): Unit {
            test(true)
            test(false)
        }
    "#,
        "entry",
    );
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"unit-ready\nunit-ready\n");
}
