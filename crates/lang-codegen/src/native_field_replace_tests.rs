//! SPEC-0246：单文件一级 class 字段 replace 的真实运行与唯一释放。
use super::{TestDirectory, analyze, emit_link_and_run, symbol};
use lang_frontend::name_resolution::SymbolKind;
use std::{fs, process::Command};

#[test]
fn field_replace_native_evaluates_once_preserves_parent_and_returns_old() {
    let output = emit_link_and_run(
        "field-exchange.ko",
        r#"
        class Holder(var state: String, val id: Int)
        fun make(): String { println("evaluate-new") return "new" + "value" }
        fun entry(): Unit {
            val holder = Holder("old" + "value", 42)
            val old = replace(&holder.state, make())
            println(old)
            println(replace(&holder.state, "last"))
            if (holder.id == 42) println("parent-live")
            println(replace(&(holder.state), "last"))
            println(replace(&holder.state, "last"))
        }
    "#,
        "entry",
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        output.stdout,
        b"evaluate-new\noldvalue\nnewvalue\nparent-live\nlast\nlast\n"
    );
}

#[test]
fn field_replace_native_copyable_snapshots_and_owned_class_return() {
    let output = emit_link_and_run(
        "field-copy.ko",
        r#"
        class Number(var state: Int)
        class Node(val value: Int)
        class Holder(var state: Node)
        fun oldNode(): Node {
            val holder = Holder(Node(1))
            val old = replace(&holder.state, Node(2))
            return old
        }
        fun entry(): Unit {
            val holder = Number(1)
            val snapshot = holder.state
            var replacement = 2
            val old = replace(&holder.state, replacement)
            { replacement = 3 }
            val current = holder.state
            if (old == 1 && snapshot == 1 && current == 2 && replacement == 3) println("snapshots")
            val returned = oldNode()
            if (returned.value == 1) println("returned-owner")
        }
    "#,
        "entry",
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(output.stdout, b"snapshots\nreturned-owner\n");
}

#[test]
fn field_replace_native_control_exit_keeps_field_initialized() {
    let output = emit_link_and_run(
        "field-control.ko",
        r#"
        class Holder(var state: String)
        fun breakCase(flag: Boolean): Unit {
            val holder = Holder("old" + "break")
            loop {
                val old = replace(&holder.state, if (flag) break else "new")
                println(old)
                break
            }
            println(replace(&holder.state, "last"))
        }
        fun returnCase(flag: Boolean): Unit {
            val holder = Holder("old" + "return")
            val old = replace(&holder.state, if (flag) return else "new")
            println(old)
            println(replace(&holder.state, "last"))
        }
        fun continueCase(): Unit {
            val holder = Holder("old" + "continue")
            var first = true
            loop {
                if (!first) break
                first = false
                val old = replace(&holder.state, continue)
            }
            println(replace(&holder.state, "last"))
        }
        fun entry(): Unit {
            breakCase(true)
            breakCase(false)
            returnCase(true)
            returnCase(false)
            continueCase()
        }
    "#,
        "entry",
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        output.stdout,
        b"oldbreak\noldbreak\nnew\noldreturn\nnew\noldcontinue\n"
    );
}

#[test]
fn field_replace_native_returns_unique_owners_and_frees_exact_pointers() {
    let llvm = lower_counted(
        r#"
        class Holder(var state: String)
        class Node(val text: String, val id: Int)
        class Nodes(var state: Node)
        fun test(flag: Boolean): Unit {
            val holder = Holder("old" + "field")
            val old = replace(&holder.state, if (flag) return else "new" + "field")
            println(old)
            println(replace(&holder.state, "last"))
        }
        fun entry(): Unit {
            test(true)
            test(false)
            val nodes = Nodes(Node("old" + "node", 1))
            val old = replace(&nodes.state, Node("new" + "node", 2))
            if (old.id == 1) println("oldnode")
        }
    "#,
    );
    // Two calls: 2 parent allocations + 3 buffers; Nodes: parent + 2 Nodes + 2 buffers.
    run_counted(&llvm, 10, b"oldfield\nnewfield\noldnode\n");
}

fn lower_counted(source: &str) -> String {
    let analysis = analyze("field-counted.ko", source);
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
    .expect("field replacement lowers to verified SSA");
    crate::llvm::render_verified_program_with_entry(&program, entry)
        .unwrap()
        .replace("@malloc(", "@counted_malloc(")
        .replace("@free(", "@counted_free(")
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
fn field_replace_native_abort_does_not_commit_or_unwind() {
    let llvm = lower_counted(
        r#"
        class Holder(var state: String)
        fun entry(): Unit {
            val holder = Holder("old" + "field")
            val old = replace(&holder.state, error("stop"))
        }
    "#,
    )
    .replace("@counted_free(", "@unexpected_free(")
    .replace("@abort(", "@expected_abort(");
    let directory = TestDirectory::create();
    let ir = directory.join("abort.ll");
    let counter = directory.join("counter.c");
    let executable = directory.join("abort");
    fs::write(&ir, llvm).unwrap();
    fs::write(&counter, r#"
#include <stdlib.h>
#include <assert.h>
static int allocations;
void *counted_malloc(size_t size) { ++allocations; assert(allocations <= 2); void *p = malloc(size); assert(p); return p; }
void unexpected_free(void *pointer) { (void)pointer; _Exit(41); }
void expected_abort(void) { assert(allocations == 2); _Exit(0); }
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
fn field_replace_native_pending_exchange_survives_inner_loop() {
    let run = emit_link_and_run(
        "field-inner-loop.ko",
        r#"
        class Holder(var state: Int)
        fun test(flag: Boolean): Unit {
            val holder = Holder(1)
            val old = replace(&holder.state, if (flag) { loop { break }
2 } else { 3 })
            val current = holder.state
            if (old == 1 && current == (if (flag) 2 else 3)) println("inner-loop")
        }
        fun entry(): Unit { test(true)
            test(false) }
        "#,
        "entry",
    );
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"inner-loop\ninner-loop\n");
}
