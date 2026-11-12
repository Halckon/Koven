//! Ordinary source object/link/run: logical callback count, length, and Resource cleanup.
use super::*;
use crate::test_support::diagnostics::assert_diagnostic;

pub(crate) mod evaluation {
//! Normal source sequencing, including a language-level negative-length Abort.
use super::*;
use crate::test_support::diagnostics::{assert_diagnostic, assert_eq_diagnostic};

pub(crate) struct EvaluationCase {
    pub(crate) label: String,
    // Single: format!("{}\n{}", case.api, case.entry).
    // Unit: package p + api; package q + imports + entry (real cross-file factory).
    pub(crate) api: String,
    pub(crate) entry: String,
    pub(crate) expected: &'static [u8],
    pub(crate) aborts: bool,
}

/// Six ordinary programs distinguish once-only operands and ordered invocations.
pub(crate) fn evaluation_cases() -> Vec<EvaluationCase> {
    let mut cases = Vec::new();
    for container in ["Array", "List"] {
        for length in [-1, 0, 3] {
            let api = format!(
                r#"fun size(): Int {{ println("size")
return {length} }}
fun trace(index: Int): Unit {{
    if (index == 0) {{ println("call-zero")
return }}
    if (index == 1) {{ println("call-one")
return }}
    if (index == 2) {{ println("call-two")
return }}
    error("unexpected callback index")
}}
fun factory(): (Int)->Int {{
    println("factory")
    return ({{ index -> trace(index)
index }})
}}"#
            );
            let check = if length == 3 {
                "if (items[2] != 2) { error(\"element\") }"
            } else {
                ""
            };
            let entry = format!(
                r#"fun entry(): Unit {{
    val items = {container}<Int>(size(), factory())
    if (items.size != {length}) {{ error("length") }}
    {check}
    println("after")
}}"#
            );
            cases.push(EvaluationCase {
                label: format!("{container}/length{length}"),
                api,
                entry,
                expected: match length {
                    -1 => b"size\n",
                    0 => b"size\nfactory\nafter\n",
                    3 => b"size\nfactory\ncall-zero\ncall-one\ncall-two\nafter\n",
                    _ => unreachable!("fixed fixture lengths"),
                },
                aborts: length < 0,
            });
        }
    }
    cases
}

/// Language-level Nothing operands abort before generation or any callback invocation.
pub(crate) fn nothing_operand_cases() -> Vec<EvaluationCase> {
    let mut cases = Vec::new();
    for container in ["Array", "List"] {
        for (label, operands) in [
            (
                "size",
                "error(\"size\"), { index -> println(\"call\")\nindex }",
            ),
            ("initializer", "3, error(\"initializer\")"),
        ] {
            cases.push(EvaluationCase {
                label: format!("{container}/Nothing-{label}"),
                api: "fun unused(): Unit {}".to_owned(),
                entry: format!("fun entry(): Unit {{ val unused = {container}<Int>({operands})\nprintln(\"after\") }}"),
                expected: b"",
                aborts: true,
            });
        }
    }
    cases
}

pub(crate) fn assert_evaluation_output(case: &EvaluationCase, output: &std::process::Output) {
    if case.aborts {
        assert_diagnostic!(!output.status.success(), "{}: {output:?}", case.label);
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            assert_eq_diagnostic!(
                output.status.signal(),
                Some(6),
                "{}: {output:?}",
                case.label
            );
        }
        // Do not use a stderr-empty oracle for SIGABRT: platform crash reporting
        // is independent of whether size/factory/callback language effects ran.
    } else {
        assert_diagnostic!(output.status.success(), "{}: {output:?}", case.label);
        assert_diagnostic!(output.stderr.is_empty(), "{}: {output:?}", case.label);
    }
    assert_eq!(output.stdout, case.expected, "{}", case.label);
}

#[test]
fn runtime_constructor_native_single_evaluates_operands_once_in_order() {
    let cases = evaluation_cases();
    assert_eq!(cases.len(), 6);
    for case in cases {
        let text = format!("{}\n{}", case.api, case.entry);
        let output = emit_link_and_run("runtime-evaluation.ko", &text, "entry");
        assert_evaluation_output(&case, &output);
    }
}

#[test]
fn runtime_constructor_native_single_nothing_operands_do_not_generate() {
    let cases = nothing_operand_cases();
    assert_eq!(cases.len(), 4);
    for case in cases {
        let text = format!("{}\n{}", case.api, case.entry);
        let output = emit_link_and_run("runtime-nothing.ko", &text, "entry");
        assert_evaluation_output(&case, &output);
    }
}
}

pub(crate) mod storable_helpers {
//! Generic Borrow Fn helpers with existing nested and nominal storable elements.
use super::*;

/// Keep the helper separate for the unit facade's real p-to-q source import.
pub(crate) fn cases() -> Vec<(RuntimeCase, String)> {
    let mut cases = Vec::new();
    for (element, declarations, constructor, projection) in [
        ("Array<Int>", "", "arrayOf", "value[0]"),
        ("List<Int>", "", "listOf", "value[0]"),
        (
            "Wrapped",
            "value class Wrapped(val number: Int)",
            "Wrapped",
            "value.number",
        ),
        (
            "Holder<Int>",
            "class Holder<T>(val number: T)",
            "Holder<Int>",
            "value.number",
        ),
    ] {
        for container in ["Array", "List"] {
            for (environment, captured, moved) in [
                ("pointer", false, false),
                ("shared", true, false),
                ("owned", true, true),
            ] {
                for explicit in [true, false] {
                    let number = if captured { "index + scale" } else { "index" };
                    let expected = if captured { 9 } else { 2 };
                    let literal = format!(
                        "{}{{ index -> println(\"call\")\n{constructor}({number}) }}",
                        if moved { "move " } else { "" }
                    );
                    let nested_check = if constructor == "arrayOf" || constructor == "listOf" {
                        "if (value.size != 1) { error(\"nested length\") }\n"
                    } else {
                        ""
                    };
                    let callee = if explicit {
                        format!("generate<{element}>")
                    } else {
                        "generate".to_owned()
                    };
                    let api = format!(
                        "fun <T> generate(size: Int, initializer: (Int)->T): {container}<T> = {container}<T>(size, initializer)"
                    );
                    let text = format!(
                        r#"{declarations}
fun verify(value: {element}): Unit {{
    {nested_check}if ({projection} != {expected}) {{ error("element value") }}
}}
fun entry(): Int {{
    val scale = 7
    val callback: (Int)->{element} = {literal}
    val items = {callee}(3, callback)
    verify(items[2])
    return items.size
}}
fun nativeEntry(): Unit {{ if (entry() != 3) {{ error("logical length") }} }}"#
                    );
                    cases.push((
                        RuntimeCase {
                            text,
                            label: format!(
                                "storable-helper/{element}/{container}/{environment}/explicit{explicit}"
                            ),
                            length: 3,
                            resource: false,
                            element,
                            container,
                            named: true,
                        },
                        api,
                    ));
                }
            }
        }
    }
    cases
}

#[test]
fn runtime_constructor_native_single_storable_helper_elements() {
    let cases = cases();
    assert_eq!(cases.len(), 48);
    for (case, api) in cases {
        let text = format!("{api}\n{}", case.text);
        let output = emit_link_and_run("runtime-storable-helper.ko", &text, "nativeEntry");
        assert_output(&case, &output);
    }
}
}

#[derive(Clone)]
pub(crate) struct RuntimeCase {
    pub(crate) text: String,
    pub(crate) label: String,
    pub(crate) length: usize,
    resource: bool,
    element: &'static str,
    container: &'static str,
    named: bool,
}

/// Separate native intent from SSA layout checks: callback side effects must execute at runtime.
pub(crate) fn cases() -> Vec<RuntimeCase> {
    let mut cases = Vec::new();
    for (element, declarations, source, bodies) in [
        (
            "Int",
            "",
            "val scale = 7",
            ["index", "index + scale", "index + scale"],
        ),
        (
            "String",
            "",
            "val scale = \"suffix\"",
            ["\"value\"", "\"value\" + scale", "\"value\" + scale"],
        ),
        (
            "Leaf",
            "class Leaf(val number: Int) { deinit() { println(\"leaf\") } }",
            "val scale = Leaf(7)",
            [
                "Leaf(index)",
                "Leaf(index + scale.number)",
                "Leaf(index + scale.number)",
            ],
        ),
        (
            "Unit",
            "fun touch(index: Int): Unit {}",
            "val scale = 7",
            [
                "touch(index)",
                "touch(index + scale)",
                "touch(index + scale)",
            ],
        ),
    ] {
        for container in ["Array", "List"] {
            for (environment, body) in bodies.iter().enumerate() {
                for named in [false, true] {
                    for length in [0, 1, 3] {
                        let literal = format!(
                            "{}{{ index -> println(\"call\")\n{body} }}",
                            if environment == 2 { "move " } else { "" }
                        );
                        let declaration = if named {
                            format!("val callback: (Int)->{element} = {literal}\n")
                        } else {
                            String::new()
                        };
                        let initializer = if named { "callback" } else { &literal };
                        let value_check = if length == 0 || element == "Unit" {
                            String::new()
                        } else {
                            format!("verify(items[{}])\n", length - 1)
                        };
                        let number = if length == 0 { 0 } else { length - 1 }
                            + if environment == 0 { 0 } else { 7 };
                        let verifier = match element {
                            "Int" => format!(
                                "fun verify(value: Int): Unit {{ if (value != {number}) {{ error(\"element value\") }} }}"
                            ),
                            "String" => format!(
                                "fun verify(value: String): Unit {{ if (value != \"{}\") {{ error(\"element value\") }} }}",
                                if environment == 0 {
                                    "value"
                                } else {
                                    "valuesuffix"
                                }
                            ),
                            "Leaf" => format!(
                                "fun verify(value: Leaf): Unit {{ if (value.number != {number}) {{ error(\"element value\") }} }}"
                            ),
                            _ => String::new(),
                        };
                        cases.push(RuntimeCase {
                            text: format!("{declarations}\n{verifier}\nfun entry(): Int {{ {source}\n{declaration}val items = {container}<{element}>({length}, {initializer})\n{value_check}return items.size }}\nfun nativeEntry(): Unit {{ if (entry() != {length}) {{ error(\"logical length\") }} }}"),
                            label: format!("{element}/{container}/env{environment}/named{named}/length{length}"),
                            length,
                            resource: element == "Leaf",
                            element,
                            container,
                            named,
                        });
                    }
                }
            }
        }
    }
    cases
}

pub(crate) fn assert_output(case: &RuntimeCase, output: &Output) {
    assert_diagnostic!(output.status.success(), "{}: {output:?}", case.label);
    assert_diagnostic!(output.stderr.is_empty(), "{}: {output:?}", case.label);
    let text = std::str::from_utf8(&output.stdout).expect("fixture stdout is UTF-8");
    let calls = text.lines().filter(|line| *line == "call").count();
    let drops = text.lines().filter(|line| *line == "leaf").count();
    assert_eq!(
        calls, case.length,
        "{}: exactly one invocation per logical element, including Unit zero stride",
        case.label
    );
    assert_eq!(
        drops,
        if case.resource { case.length + 1 } else { 0 },
        "{}: Resource deinitialization total matches elements plus capture source",
        case.label
    );
    assert_eq!(
        text.lines().count(),
        calls + drops,
        "{}: unexpected side effects",
        case.label
    );
}

#[test]
fn runtime_constructor_native_single_matrix_executes_logical_callbacks() {
    let cases = cases();
    assert_eq!(cases.len(), 144);
    for case in &cases {
        let output = emit_link_and_run("runtime-constructor.ko", &case.text, "nativeEntry");
        assert_output(case, &output);
    }
}

// Ordinary observable ordering; no allocator substitution, fault injection, or sanitizer.
pub(crate) fn ordered_resource_cases() -> Vec<(String, &'static str)> {
    let mut cases = Vec::new();
    for container in ["Array", "List"] {
        for (initializer, expected) in [
            (
                "{ index -> make(index) }",
                "call-zero\ncall-one\ncall-two\nafter\ntwo\none\nzero\nenv\n",
            ),
            (
                "{ index -> println(scale.name)\nmake(index) }",
                "env\ncall-zero\nenv\ncall-one\nenv\ncall-two\nafter\ntwo\none\nzero\nenv\n",
            ),
            (
                "move { index -> println(scale.name)\nmake(index) }",
                "env\ncall-zero\nenv\ncall-one\nenv\ncall-two\nenv\nafter\ntwo\none\nzero\n",
            ),
        ] {
            let text = format!(
                r#"class Resource(val name: String) {{ deinit() {{ println(this.name) }} }}
fun make(index: Int): Resource {{
    if (index == 0) {{ println("call-zero")
return Resource("zero") }}
    if (index == 1) {{ println("call-one")
return Resource("one") }}
    println("call-two")
    return Resource("two")
}}
fun entry(): Unit {{
    val scale = Resource("env")
    val items = {container}<Resource>(3, {initializer})
    println("after")
    if (items.size != 3) {{ error("length") }}
}}"#
            );
            cases.push((text, expected));
        }
    }
    cases
}

#[test]
fn runtime_constructor_native_single_orders_callbacks_elements_and_environment_cleanup() {
    for (text, expected) in ordered_resource_cases() {
        let output = emit_link_and_run("runtime-order.ko", &text, "entry");
        assert_diagnostic!(output.status.success(), "{output:?}");
        assert_eq!(output.stdout, expected.as_bytes());
        assert_diagnostic!(output.stderr.is_empty(), "{output:?}");
    }
}

/// Exercise the same value/count oracle through a generic Borrow Fn helper.
/// The API is separate so the unit entry can place it in another source file.
pub(crate) fn helper_cases() -> Vec<(RuntimeCase, String)> {
    cases()
        .into_iter()
        .filter(|case| case.named && case.length == 3)
        .flat_map(|case| {
            [true, false].into_iter().map(move |explicit| {
                let mut instance = case.clone();
                let constructor = format!("{}<{}>(3, callback)", case.container, case.element);
                assert_eq!(instance.text.matches(&constructor).count(), 1);
                let callee = if explicit {
                    format!("generate<{}>", case.element)
                } else {
                    "generate".to_owned()
                };
                instance.text = instance.text.replacen(&constructor, &format!("{callee}(3, callback)"), 1);
                let api = format!(
                    "fun <T> generate(size: Int, initializer: (Int)->T): {}<T> = {}<T>(size, initializer)",
                    case.container, case.container,
                );
                instance.label = format!("generic-helper/explicit{explicit}/{}", case.label);
                (instance, api)
            })
        })
        .collect()
}

#[test]
fn runtime_constructor_native_single_generic_helper_preserves_element_and_environment() {
    let cases = helper_cases();
    assert_eq!(cases.len(), 48);
    for (case, api) in cases {
        let text = format!("{api}\n{}", case.text);
        let output = emit_link_and_run("runtime-helper.ko", &text, "nativeEntry");
        assert_output(&case, &output);
    }
}

pub(crate) mod root_primitive_tests {
//! SPEC-0244：源码原语真实 link/run 与精确 owner 释放。
use super::super::{TestDirectory, UnitSource, emit_link_and_run};
use crate::test_support::diagnostics::assert_diagnostic;
use lang_frontend::name_resolution::SymbolKind;
use std::{fs, process::Command};

#[test]
fn root_primitive_native_string_exchange_and_source_order() {
    let output = emit_link_and_run(
        "root-exchange.ko",
        r#"
        fun make(): String { println("evaluate-new"); return "new" + "value" }
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
    assert_diagnostic!(output.status.success(), "{output:?}");
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
    assert_diagnostic!(output.status.success(), "{output:?}");
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
    assert_diagnostic!(output.status.success(), "{output:?}");
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
    let analysis = UnitSource::analyze("root-counted.ko", source)
        .expect("original source must pass strict Unit admission");
    analysis.assert_frontend_clean();
    let (program, entry) = analysis
        .lower_entry(analysis.declaration("entry", SymbolKind::Function))
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
    let analysis = UnitSource::analyze(
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
    )
    .expect("original source must pass strict Unit admission");
    analysis.assert_frontend_clean();
    let (program, entry) = analysis
        .lower_entry(analysis.declaration("entry", SymbolKind::Function))
        .expect("conditional return keeps old root until normal commit");
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry)
        .unwrap()
        .replace("@malloc(", "@counted_malloc(")
        .replace("@free(", "@counted_free(");
    run_counted(&llvm, 3, b"oldvalue\nnewvalue\n");
}

#[test]
fn root_primitive_native_abort_never_commits_or_drops_the_old_root() {
    let analysis = UnitSource::analyze(
        "root-abort-counts.ko",
        r#"
        fun entry(): Unit {
            var target = "old" + "value"
            val old = replace(&target, error("stop"))
        }
    "#,
    )
    .expect("original source must pass strict Unit admission");
    analysis.assert_frontend_clean();
    let (program, entry) = analysis
        .lower_entry(analysis.declaration("entry", SymbolKind::Function))
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
    assert!(
        run.stdout.is_empty() && run.stderr.starts_with(b"abort: "),
        "{run:?}"
    );
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
}
