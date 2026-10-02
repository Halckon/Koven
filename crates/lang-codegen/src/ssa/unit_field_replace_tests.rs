//! 一级普通 class 字段 replace 的 unit SSA 与真实本机所有权回归。
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

use super::{
    LoweringError, LoweringErrorKind,
    model::{Function, FunctionId, Operation, Program},
    unit_lower::{constant::lower_constant_unit_with_entry, lower_scalar_unit_with_entry},
    unit_lower_test_support::{analyze, declaration, parsed},
};
use lang_frontend::{
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::check_compilation_unit_constant_ownership,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};

fn lower(text: &str, constants: bool) -> Result<(Program, FunctionId), LoweringError> {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "test/field.ko",
        &format!("package test\n{text}"),
    );
    let inputs = [SourceUnitInput::new("root", "test/field.ko", source, &file)];
    let (name_environment, environment) = standard_environments();
    if !constants {
        let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &environment);
        return lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &environment,
            &typed,
            &owned,
            declaration(&names, "test", "entry"),
        );
    }
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &environment)
        .unwrap()
        .validate_constants()
        .unwrap();
    let owned =
        check_compilation_unit_constant_ownership(&sources, &inputs, &names, &environment, &typed)
            .unwrap()
            .validate()
            .unwrap();
    lower_constant_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &environment,
        &typed,
        &owned,
        declaration(&names, "test", "entry"),
    )
}

fn entry(program: &Program) -> &Function {
    program.modules[0]
        .functions
        .iter()
        .find(|function| function.name.contains("test.entry."))
        .unwrap()
}
fn count(function: &Function, prefix: &str) -> usize {
    function
        .instructions
        .iter()
        .filter(|instruction| format!("{:?}", instruction.operation).starts_with(prefix))
        .count()
}

#[test]
fn unit_field_replace_val_owner_survives_and_old_owner_returns() {
    for constants in [false, true] {
        let (program, _) = lower(
            r#"
            class Holder(var text: String, val tag: Int)
            fun entry(): String {
                val holder = Holder("old", 7)
                val replacement = "new"
                val old = replace(&holder.text, replacement)
                println(old)
                val tag = holder.tag
                return old
            }
        "#,
            constants,
        )
        .expect("field commit preserves its val parent and returns the unique old owner");
        let function = entry(&program);
        assert_eq!(count(function, "HeapFieldExchange"), 1);
        assert_eq!(count(function, "RootReplace"), 0);
        assert_eq!(
            count(function, "Drop"),
            1,
            "only the parent drops before returning old"
        );
    }
}

#[test]
fn unit_field_replace_conditional_rhs_preserves_pending_owner_and_loan() {
    let (program, _) = lower(
        r#"
        class Holder(var text: String, val tag: Int)
        fun entry(own flag: Boolean): String {
            val holder = Holder("old", 7)
            val old = replace(&holder.text, if (flag) { "yes" } else { "no" })
            val tag = holder.tag
            return old
        }
    "#,
        true,
    )
    .expect("field owner and exclusive loan survive conditional CFG rebindings");
    assert_eq!(count(entry(&program), "HeapFieldExchange"), 1);
}

#[test]
fn unit_field_replace_return_prefix_ends_loan_before_parent_cleanup() {
    let source = r#"
        class Holder(var text: String)
        fun entry(): String {
            val holder = Holder("old")
            val unused = replace(&holder.text, return "early")
            return unused
        }
    "#;
    assert_eq!(
        lower(source, false)
            .err()
            .expect("source is outside this lowering boundary")
            .kind,
        LoweringErrorKind::UnsupportedNode
    );
    let (program, _) =
        lower(source, true).expect("constant-unit return cancels the pending field commit");
    let function = entry(&program);
    assert_eq!(count(function, "HeapFieldExchange"), 0);
    let end = function
        .instructions
        .iter()
        .position(|instruction| matches!(instruction.operation, Operation::BorrowEnd { .. }))
        .unwrap();
    let drop = function
        .instructions
        .iter()
        .position(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
        .unwrap();
    assert!(end < drop);
}

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "koven-unit-field-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn run_native(source: &str, constants: bool, allocations: usize, stdout: &[u8]) {
    let (program, entry) = lower(source, constants).expect("unit field source lowers");
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry)
        .unwrap()
        .replace("@malloc(", "@counted_malloc(")
        .replace("@free(", "@counted_free(");
    let counter = format!(
        r#"
#include <stdlib.h>
#include <assert.h>
static void *live[{allocations}];
static int allocated, released;
void *counted_malloc(size_t n) {{
    assert(n && allocated < {allocations});
    void *p = malloc(n); assert(p);
    for (int i = 0; i < allocated; ++i) assert(live[i] != p);
    live[allocated++] = p; return p;
}}
void counted_free(void *p) {{
    assert(p);
    for (int i = 0; i < allocated; ++i) if (live[i] == p) {{
        live[i] = 0; ++released; free(p); return;
    }}
    abort();
}}
__attribute__((destructor)) static void verify(void) {{
    assert(allocated == {allocations} && released == {allocations});
    for (int i = 0; i < allocated; ++i) assert(!live[i]);
}}
"#
    );
    link_and_run(&llvm, &counter, stdout);
}
fn link_and_run(llvm: &str, counter: &str, stdout: &[u8]) {
    let directory = Directory::new();
    let ir = directory.0.join("field.ll");
    let c = directory.0.join("counter.c");
    let executable = directory.0.join("field");
    fs::write(&ir, llvm).unwrap();
    fs::write(&c, counter).unwrap();
    let link = Command::new(crate::test_support::ir_clang())
        .arg(&ir)
        .arg(&c)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(link.status.success(), "{link:?}");
    let run = Command::new(executable).output().unwrap();
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, stdout);
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[test]
fn unit_field_replace_native_new_once_parent_live_and_exact_pointer_frees() {
    for constants in [false, true] {
        run_native(
            r#"
            class Holder(var text: String, val tag: Int)
            fun make(): String { println("new-once") return "new" + "value" }
            fun entry(): Unit {
                val holder = Holder("old" + "value", 7)
                val old = replace(&holder.text, make())
                println(old)
                if (holder.tag == 7) { println("parent-live") }
                println(replace(&holder.text, "last"))
            }
        "#,
            constants,
            3,
            b"new-once\noldvalue\nparent-live\nnewvalue\n",
        );
    }
}

#[test]
fn unit_field_replace_native_returned_old_is_borrowed_and_freed_once() {
    for constants in [false, true] {
        run_native(
            r#"
            class Holder(var text: String, val tag: Int)
            fun extract(): String {
                val holder = Holder("old" + "return", 7)
                val old = replace(&holder.text, "new" + "value")
                if (holder.tag == 7) { println("parent-live") }
                return old
            }
            fun entry(): Unit { val old = extract()
                println(old)
                println(old)
            }
        "#,
            constants,
            3,
            b"parent-live\noldreturn\noldreturn\n",
        );
    }
}

#[test]
fn unit_field_replace_native_copyable_values_are_snapshots() {
    for constants in [false, true] {
        run_native(
            r#"
            class Holder(var number: Int)
            fun entry(): Unit {
                val holder = Holder(7)
                var next = 11
                val snapshot = holder.number
                val old = replace(&holder.number, next)
                next = 19
                val current = holder.number
                if (old == 7 && snapshot == 7 && current == 11 && next == 19) {
                    println("snapshots")
                }
            }
        "#,
            constants,
            1,
            b"snapshots\n",
        );
    }
}

#[test]
fn unit_field_replace_native_break_continue_leave_field_initialized() {
    for exit in ["break", "continue"] {
        let source = format!(
            r#"
            class Holder(var text: String)
            fun test(flag: Boolean): Unit {{
                val holder = Holder("old" + "value")
                var once = true
                loop {{
                    if (!once) {{ break }}
                    once = false
                    val old = replace(&holder.text, if (flag) {{ {exit} }} else {{ "new" + "value" }})
                    println(old)
                    break
                }}
                println(replace(&holder.text, "last"))
            }}
            fun entry(): Unit {{ test(true)
                test(false)
            }}
        "#
        );
        assert_eq!(
            lower(&source, false)
                .err()
                .expect("source is outside this lowering boundary")
                .kind,
            LoweringErrorKind::UnsupportedNode
        );
        run_native(&source, true, 5, b"oldvalue\noldvalue\nnewvalue\n");
    }
}

#[test]
fn unit_field_replace_native_return_before_commit_frees_exact_original_owners() {
    run_native(
        r#"
        class Holder(var text: String)
        fun test(flag: Boolean): Unit {
            val holder = Holder("old" + "value")
            val old = replace(&holder.text, if (flag) return else "new" + "value")
            println(old)
            println(replace(&holder.text, "last"))
        }
        fun entry(): Unit { test(true)
            test(false)
        }
    "#,
        true,
        5,
        b"oldvalue\nnewvalue\n",
    );
}

#[test]
fn unit_field_replace_native_abort_never_unwinds_or_commits() {
    for constants in [false, true] {
        let (program, entry_id) = lower(
            r#"
            class Holder(var text: String)
            fun entry(): Unit {
                val holder = Holder("old" + "value")
                val old = replace(&holder.text, error("stop"))
            }
        "#,
            constants,
        )
        .expect("abort lowers only the evaluated field-call prefix");
        assert_eq!(count(entry(&program), "HeapFieldExchange"), 0);
        let function = entry(&program);
        let field_prefix = function
            .instructions
            .iter()
            .position(|instruction| matches!(instruction.operation, Operation::FieldPlace { .. }))
            .expect("exclusive field prefix is evaluated before abort");
        assert!(
            function.instructions[field_prefix..]
                .iter()
                .all(|instruction| { !matches!(instruction.operation, Operation::Drop { .. }) }),
            "abort never cleans up the parent or old field; concat input literals may drop earlier"
        );
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry_id)
            .unwrap()
            .replace("@malloc(", "@counted_malloc(")
            .replace("@free(", "@unexpected_free(")
            .replace("@abort(", "@expected_abort(");
        link_and_run(
            &llvm,
            r#"
#include <stdlib.h>
#include <assert.h>
static void *live[2];
static int allocated;
void *counted_malloc(size_t n) { assert(n && allocated < 2); void *p = malloc(n); assert(p); live[allocated++] = p; return p; }
void unexpected_free(void *p) { (void)p; _Exit(41); }
void expected_abort(void) { assert(allocated == 2 && live[0] && live[1] && live[0] != live[1]); _Exit(0); }
__attribute__((destructor)) static void unexpected_return(void) { _Exit(42); }
"#,
            b"",
        );
    }
}

#[test]
fn unit_field_replace_existing_storage_families_keep_unique_return_owners() {
    for (declarations, ty, first, second) in [
        ("", "Int", "7", "11"),
        ("", "Boolean", "true", "false"),
        (
            "class Item(val text: String)",
            "Item",
            "Item(\"old\")",
            "Item(\"new\")",
        ),
        (
            "value class Item(val text: String)",
            "Item",
            "Item(\"old\")",
            "Item(\"new\")",
        ),
        (
            "value class Item(val number: Int)",
            "Item",
            "Item(7)",
            "Item(11)",
        ),
        (
            "enum class Item { Full(text: String), Empty }",
            "Item",
            "Item.Full(\"old\")",
            "Item.Full(\"new\")",
        ),
        (
            "value class Item(val number: Int)",
            "Box<Item>",
            "Box(Item(7))",
            "Box(Item(11))",
        ),
        ("", "Rc<Int>", "Rc(7)", "Rc(11)"),
        ("", "List<Int>", "listOf(7)", "listOf(11)"),
        ("", "Array<String>", "arrayOf(\"old\")", "arrayOf(\"new\")"),
    ] {
        let source = format!(
            "{declarations}\nclass Holder(var item: {ty}, val tag: Int)\nfun entry(): {ty} {{\nval holder = Holder({first}, 7)\nval old = replace(&holder.item, {second})\nval tag = holder.tag\nreturn old\n}}"
        );
        for constants in [false, true] {
            let (program, _) = lower(&source, constants)
                .unwrap_or_else(|error| panic!("{ty}, constants={constants}: {error:?}"));
            assert_eq!(count(entry(&program), "HeapFieldExchange"), 1, "{ty}");
            assert_eq!(count(entry(&program), "RootReplace"), 0, "{ty}");
        }
    }
}

#[test]
fn unit_field_replace_nested_same_parent_loans_remain_explicitly_unsupported() {
    for constants in [false, true] {
        let error = lower(
            r#"
            class Holder(var first: String, var second: String)
            fun entry(): String {
                val holder = Holder("first", "second")
                return replace(&holder.first, replace(&holder.second, "third"))
            }
        "#,
            constants,
        )
        .err()
        .expect("simultaneous sibling loans need projection-aware SSA aliasing");
        assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    }
}

#[test]
fn unit_field_replace_keeps_nested_generic_non_owning_and_swap_boundaries() {
    for source in [
        "class Inner(var text: String)\nclass Outer(var inner: Inner)\nfun entry(): String {\nval outer = Outer(Inner(\"old\"))\nreturn replace(&outer.inner.text, \"new\")\n}",
        "class Holder<T>(var item: T)\nfun entry(): String {\nval holder = Holder(\"old\")\nreturn replace(&holder.item, \"new\")\n}",
        "class Holder(var item: Rc<Int>?)\nfun entry(): Rc<Int>? {\nval holder = Holder(null)\nreturn replace(&holder.item, Rc(11))\n}",
        "class Holder(var text: String)\nfun entry(inout holder: Holder): String = replace(&holder.text, \"new\")",
        "class Holder(var first: Int, var second: Int)\nfun entry(): Int {\nval holder = Holder(1, 2)\nreturn replace(&holder.first, holder.second)\n}",
        "class Holder(var first: String, var second: String)\nfun entry(): String {\nval holder = Holder(\"old\", \"new\")\nreturn replace(&holder.first, holder.second.clone())\n}",
        "class Holder(var first: String, var second: String)\nfun entry(): Unit {\nval holder = Holder(\"first\", \"second\")\nswap(&holder.first, &holder.second)\n}",
    ] {
        for constants in [false, true] {
            let error = lower(source, constants)
                .err()
                .expect("source is outside this lowering boundary");
            assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode, "{source}");
        }
    }
}

#[test]
fn unit_field_replace_cross_file_input_order_is_deterministic() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\nfun make(): String = \"new\"",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/entry.ko",
        "package q\nclass Holder(var item: String, val tag: Int)\nfun entry(): String {\nval holder = Holder(\"old\", 7)\nval old = replace(&holder.item, p.make())\nval tag = holder.tag\nreturn old\n}",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/entry.ko", consumer_source, &consumer),
    ];
    let reversed = [inputs[1], inputs[0]];
    let (names_environment, environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &names_environment, &environment);
    let (reverse_names, reverse_typed, reverse_owned) =
        analyze(&sources, &reversed, &names_environment, &environment);
    let (forward, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &environment,
        &typed,
        &owned,
        declaration(&names, "q", "entry"),
    )
    .unwrap();
    let (reverse, _) = lower_scalar_unit_with_entry(
        &sources,
        &reversed,
        &reverse_names,
        &environment,
        &reverse_typed,
        &reverse_owned,
        declaration(&reverse_names, "q", "entry"),
    )
    .unwrap();
    assert_eq!(
        super::render::render_program(&forward),
        super::render::render_program(&reverse)
    );
}
