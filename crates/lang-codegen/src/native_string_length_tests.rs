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

pub(crate) mod constant_tests {
    //! SPEC-0209: source-to-native constant acceptance, including actual owner lifetimes.
    use super::super::*;

    #[test]
    fn constant_types_run_in_every_supported_namespace() {
        let values = [
            ("Boolean", "true"),
            ("Byte", "127"),
            ("Short", "32767"),
            ("Int", "2147483647"),
            ("Long", "9223372036854775807L"),
            ("UByte", "255u"),
            ("UShort", "65535u"),
            ("UInt", "4294967295u"),
            ("ULong", "18446744073709551615uL"),
        ];
        let mut declarations = values
            .iter()
            .enumerate()
            .map(|(index, (ty, literal))| format!("const val V{index}: {ty} = {literal}\n"))
            .collect::<String>();
        declarations.push_str("const val LETTER: Char = '文'\nconst val OTHER: Char = '中'\n");
        for (prefix, root) in [
            ("", declarations.clone()),
            ("Config.", format!("object Config {{ {declarations} }}")),
            (
                "Config.",
                format!("class Config {{ companion {{ {declarations} }} }}"),
            ),
            (
                "Config.",
                format!("value class Config(val item: Int) {{ companion {{ {declarations} }} }}"),
            ),
            (
                "Config.",
                format!("interface Config {{ companion {{ {declarations} }} }}"),
            ),
            (
                "Config.",
                format!("enum class Config {{ One; companion {{ {declarations} }} }}"),
            ),
        ] {
            let checks = values
            .iter()
            .enumerate()
            .map(|(index, (ty, literal))| {
                format!("val expected{index}: {ty} = {literal}\nif ({prefix}V{index} == expected{index}) {{ println(\"{index}\") }}\n")
            })
            .collect::<String>();
            let source = format!(
                "{root}\nfun letter(): Char = {prefix}LETTER\nfun entry(): Unit {{\n{checks}if (letter() == {prefix}LETTER && letter() != {prefix}OTHER) {{ println(\"char\") }}\n}}"
            );
            let run = emit_link_and_run("constant-matrix.ko", &source, "entry");
            assert!(run.status.success(), "{root}: {run:?}");
            assert_eq!(run.stdout, b"0\n1\n2\n3\n4\n5\n6\n7\n8\nchar\n", "{root}");
            assert!(run.stderr.is_empty(), "{run:?}");
        }
    }

    #[test]
    fn constants_coexist_with_argument_entry_and_repeatable_objects() {
        let analysis = UnitSource::analyze(
            "constant-argv.ko",
            r#"
const val INDEX: Int = 0
object Config { const val TEXT = "中" + "文" }
fun main(args: Array<String>): Unit {
    val text = println(Config.TEXT)
    val argument = println(args[INDEX])
}
"#,
        )
        .expect("original source must pass strict Unit admission");
        analysis.assert_frontend_clean();
        let directory = TestDirectory::create();
        let mut objects = Vec::new();
        for index in 0..2 {
            let object = directory.join(&format!("entry-{index}.o"));
            analysis
                .emit(
                    NativeUnitEntry::BorrowedArguments(
                        analysis.declaration("main", SymbolKind::Function),
                    ),
                    &object,
                )
                .expect("constant declarations must coexist with borrowed argv");
            objects.push(fs::read(&object).unwrap());
            let executable = directory.join(&format!("entry-{index}"));
            let linked = Command::new(crate::test_support::clang())
                .arg(&object)
                .arg("-o")
                .arg(&executable)
                .output()
                .unwrap();
            assert!(linked.status.success(), "{linked:?}");
            let run = Command::new(&executable).arg("参数").output().unwrap();
            assert!(run.status.success(), "{run:?}");
            assert_eq!(run.stdout, "中文\n参数\n".as_bytes());
            assert!(run.stderr.is_empty(), "{run:?}");
        }
        assert_eq!(
            objects[0], objects[1],
            "the same analysis must emit identical object bytes"
        );
    }

    #[test]
    fn constant_analysis_mismatch_and_invalid_values_never_write_objects() {
        let first = UnitSource::analyze(
            "constant.ko",
            "const val VALUE = 1\nfun entry(): Unit { val result = VALUE }",
        )
        .expect("original source must pass strict Unit admission");
        let second = UnitSource::analyze(
            "constant.ko",
            "const val VALUE = 2\nfun entry(): Unit { val result = VALUE }",
        )
        .expect("original source must pass strict Unit admission");
        let directory = TestDirectory::create();
        for (typed, owned) in [(&first, &second), (&second, &second)] {
            let output = directory.join("foreign.o");
            let error = first
                .emit_with_products_from(
                    typed,
                    owned,
                    first.declaration("entry", SymbolKind::Function),
                    &output,
                )
                .expect_err("same IDs cannot authorize values from another analysis");
            assert_eq!(error.kind(), NativeObjectErrorKind::MismatchedAnalysis);
            assert!(!output.exists());
        }
        let output = directory.join("invalid.o");
        fs::write(directory.join("keep"), b"unchanged sibling").unwrap();
        let before = directory.snapshot();
        let Err(invalid) = UnitSource::analyze(
            "overflow.ko",
            "const val VALUE: Byte = 128\nfun entry(): Unit {}",
        ) else {
            panic!("invalid constant facts must not acquire an owned Unit capability");
        };
        assert!(
            invalid
                .report
                .diagnostics
                .iter()
                .any(|(stage, _)| *stage == "types")
        );
        assert!(
            !invalid
                .report
                .stages
                .iter()
                .any(|(stage, _)| *stage == "ownership")
        );
        assert_eq!(directory.snapshot(), before);
        assert!(!output.exists());
    }

    #[test]
    fn string_constant_owners_drop_once_without_allocating_literal_buffers() {
        let analysis = UnitSource::analyze(
            "constant-owners.ko",
            r#"
const val TEXT = "中" + "文"
fun make(): String = TEXT
fun take(own text: String): String = text
fun view(text: String): Unit { println(text) }
fun entry(): Unit {
    val first = view((TEXT))
    val joined = TEXT + TEXT
    val second = println(joined)
    if (make() == TEXT) { println(TEXT) }
    val transferred = take(TEXT)
    val third = view(transferred)
    val dynamic = take(TEXT + TEXT)
    val fourth = view(dynamic)
}
"#,
        )
        .expect("original source must pass strict Unit admission");
        analysis.assert_frontend_clean();
        let (program, entry) = analysis
            .lower_entry(analysis.declaration("entry", SymbolKind::Function))
            .expect("constant owner source must verify");
        let string = program.modules[0]
            .types
            .iter()
            .position(|ty| *ty == SsaTypeKind::StringOwner)
            .unwrap();
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        let llvm = llvm
            .replace("@malloc(", "@counted_malloc(")
            .replace("@free(", "@counted_free(");
        let mut instrumented = String::new();
        let mut sites = 0;
        for line in llvm.lines() {
            instrumented.push_str(line);
            instrumented.push('\n');
            if line.contains(&format!("call void @koven.drop.t{string}(")) {
                sites += 1;
                instrumented.push_str("  call void @counted_drop()\n");
            }
        }
        assert!(sites > 0, "instrumentation must reach String drop calls");
        instrumented.push_str("declare void @counted_drop()\n");
        let directory = TestDirectory::create();
        let ir = directory.join("owners.ll");
        let counter = directory.join("counter.c");
        let executable = directory.join("owners");
        fs::write(&ir, instrumented).unwrap();
        fs::write(
            &counter,
            r#"
#include <stdlib.h>
#include <assert.h>
static void *live[2];
static int allocations, releases, drops;
void counted_drop(void) { ++drops; }
void *counted_malloc(size_t size) {
    assert(allocations < 2);
    void *value = malloc(size);
    assert(value);
    live[allocations++] = value;
    return value;
}
void counted_free(void *value) {
    for (int i = 0; i < 2; ++i) {
        if (live[i] == value) {
            live[i] = 0;
            ++releases;
            free(value);
            return;
        }
    }
    abort();
}
__attribute__((destructor)) static void verify_counts(void) {
    /* Nine literal owners + two concat owners; only concat buffers allocate. */
    assert(drops == 11);
    assert(allocations == 2 && releases == 2 && !live[0] && !live[1]);
}
"#,
        )
        .unwrap();
        let linked = Command::new(crate::test_support::ir_clang())
            .arg(&ir)
            .arg(&counter)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        assert!(linked.status.success(), "{linked:?}");
        let run = Command::new(&executable).output().unwrap();
        assert!(run.status.success(), "{run:?}");
        assert_eq!(
            run.stdout,
            "中文\n中文中文\n中文\n中文\n中文中文\n".as_bytes()
        );
        assert!(run.stderr.is_empty(), "{run:?}");
    }
}

pub(crate) mod field_replace_tests {
    //! SPEC-0246：单文件一级 class 字段 replace 的真实运行与唯一释放。
    use super::super::{TestDirectory, UnitSource, emit_link_and_run};
    use crate::test_support::diagnostics::assert_diagnostic;
    use lang_frontend::name_resolution::SymbolKind;
    use std::{fs, process::Command};

    #[test]
    fn field_replace_native_evaluates_once_preserves_parent_and_returns_old() {
        let output = emit_link_and_run(
            "field-exchange.ko",
            r#"
        class Holder(var state: String, val id: Int)
        fun make(): String { println("evaluate-new"); return "new" + "value" }
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
        assert_diagnostic!(output.status.success(), "{output:?}");
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
        assert_diagnostic!(output.status.success(), "{output:?}");
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
        assert_diagnostic!(output.status.success(), "{output:?}");
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
        let analysis = UnitSource::analyze("field-counted.ko", source)
            .expect("original source must pass strict Unit admission");
        analysis.assert_frontend_clean();
        let (program, entry) = analysis
            .lower_entry(analysis.declaration("entry", SymbolKind::Function))
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
        assert!(
            run.stdout.is_empty() && run.stderr.starts_with(b"abort: "),
            "{run:?}"
        );
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
}
