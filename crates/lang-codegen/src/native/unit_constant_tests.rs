#[path = "unit_const_handoff_contract_tests.rs"]
mod handoff_contracts;

#[path = "unit_numeric_literal_tests.rs"]
pub(crate) mod numeric_literal_tests;

use super::{Command, TestDirectory, assert_no_sibling_temporary, fs};
use crate::{NativeObjectErrorKind, NativeUnitEntry, emit_native_constant_unit_object};
use lang_frontend::{
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::check_compilation_unit_constant_ownership,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};

mod owners {
//! 动态计数只注入测试 LLVM；生产 owner/runtime ABI 不变。
use super::*;

#[test]
fn constant_and_literal_string_owners_have_matching_runtime_cleanup() {
    for value in ["TEXT", "\"中文\""] {
        let provider = format!(
            "package p\nconst val TEXT = \"中\" + \"文\"\nfun make(): String = {value}\nfun take(own text: String): String = text\nfun view(text: String): Unit {{ println(text) }}"
        );
        let consumer = format!(
            "package q\nimport p.TEXT\nfun entry(): Unit {{\nval first = p.view(({value}))\nval joined = {value} + {value}\nval second = println(joined)\nif (p.make() == {value}) {{ println({value}) }}\nval transferred = p.take({value})\nval third = p.view(transferred)\nval dynamic = p.take({value} + {value})\nval fourth = p.view(dynamic)\n}}"
        );
        let run = run_counted_strings(&provider, &consumer, 11, 2, false);
        assert!(run.status.success(), "{value}: {run:?}");
        assert_eq!(
            run.stdout,
            "中文\n中文中文\n中文\n中文\n中文中文\n".as_bytes()
        );
        assert!(run.stderr.is_empty(), "{run:?}");
    }
}

#[test]
fn abort_does_not_unwind_a_pending_constant_string_owner() {
    for value in ["p.TEXT", "\"中文\""] {
        let consumer = format!(
            "package q\nfun entry(): Unit {{ val pending = p.view_pair({value} + {value}, if (p.FLAG) {{ error({value}) }} else {{ true }}) }}"
        );
        let run = run_counted_strings(
            "package p\nconst val TEXT = \"中文\"\nconst val FLAG = true\nfun view_pair(text: String, own flag: Boolean): Unit {}",
            &consumer,
            2,
            1,
            true,
        );
        assert!(
            run.status.success(),
            "{value}: counted Abort must check state before exiting: {run:?}"
        );
        assert!(
            run.stdout.is_empty() && run.stderr.starts_with(b"abort: "),
            "{run:?}"
        );
    }
}

#[test]
fn return_cleans_a_pending_constant_string_owner() {
    let run = run_counted_strings(
        "package p\nconst val TEXT = \"中文\"\nconst val FLAG = true\nfun view_pair(text: String, own flag: Boolean): Unit { println(\"unexpected call\") }",
        "package q\nfun entry(): Unit { val pending = p.view_pair(p.TEXT + p.TEXT, if (p.FLAG) { return } else { true }) }",
        3,
        1,
        false,
    );
    assert!(
        run.status.success(),
        "pending concat must be released on return: {run:?}"
    );
    assert!(run.stdout.is_empty() && run.stderr.is_empty(), "{run:?}");
}

#[test]
fn loop_exits_clean_value_receiver_and_pending_string_argument() {
    for value in ["p.TEXT", "\"中文\""] {
        for mode in ["", "own "] {
            for exit in ["break", "continue"] {
                let provider = format!(
                    "package p\nconst val TEXT = \"中文\"\nconst val FLAG = true\nvalue class Host(val text: String) {{ own fun consume({mode}argument: String, own flag: Boolean): Unit {{ println(\"unexpected call\") }} }}"
                );
                // Continue 回到有界循环头；receiver 和实参各持有一个实际分配的 concat。
                let expected_turns = if exit == "continue" { 2 } else { 1 };
                let consumer = format!(
                    "package q\nimport p.Host\nfun entry(): Unit {{ var turns = 0\nloop {{ turns = turns + 1\nif (turns == 2) {{ break }}\nval host = Host({value} + {value})\nval pending = host.consume({value} + {value}, if (p.FLAG) {{ {exit} }} else {{ true }})\nval unexpected = println(\"unexpected continuation\")\nbreak }}\nif (turns != {expected_turns}) {{ println(\"wrong loop target\") }}\nprintln(\"done\") }}"
                );
                let run = run_counted_strings(&provider, &consumer, 7, 2, false);
                assert!(run.status.success(), "{value}/{mode}/{exit}: {run:?}");
                assert_eq!(run.stdout, b"done\n", "{value}/{mode}/{exit}: {run:?}");
                assert!(run.stderr.is_empty(), "{run:?}");
            }
        }
    }
}

#[test]
fn implicit_receiver_cleanup_matches_call_commit_at_runtime() {
    for value in ["p.TEXT", "\"中文\""] {
        for mode in ["", "own "] {
            for (flag, drops, expected) in [
                ("true", 7, "done\n"),
                ("false", 8, "中文中文\ncommitted\ndone\n"),
            ] {
                let provider = format!(
                    "package p\nconst val TEXT = \"中文\"\nvalue class Host(val text: String) {{ own fun consume({mode}argument: String, own flag: Boolean): Unit {{ println(argument) }}\nown fun relay(own flag: Boolean): Unit {{ val pending = consume({value} + {value}, if (flag) {{ return }} else {{ true }})\nval after = println(\"committed\") }} }}"
                );
                let consumer = format!(
                    "package q\nimport p.Host\nfun entry(): Unit {{ val host = Host({value} + {value})\nval done = host.relay({flag})\nval marker = println(\"done\") }}"
                );
                let run = run_counted_strings(&provider, &consumer, drops, 2, false);
                assert!(run.status.success(), "{value}/{mode}/{flag}: {run:?}");
                assert_eq!(run.stdout, expected.as_bytes(), "{run:?}");
                assert!(run.stderr.is_empty(), "{run:?}");
            }
        }
    }
}

#[test]
fn conditional_receiver_cleanup_matches_copyability_and_call_commit() {
    for receiver in ["consume", "this.consume"] {
        for (field, constructor, allocations, receiver_drops) in [
            ("String", "Host(p.TEXT + p.TEXT)", 2, 3),
            ("Int", "Host(0)", 1, 0),
        ] {
            for mode in ["", "own "] {
                for (flag, extra, expected) in [
                    ("true", 1, "done\n"),
                    ("false", 0, "中文中文\nolder\ndone\n"),
                ] {
                    let provider = format!(
                        "package p\nconst val TEXT = \"中文\"\ninterface Relay {{ own fun consume({mode}argument: String, own flag: Boolean): Unit {{ println(argument) }}\nown fun relay(own flag: Boolean): Unit {{ val older = \"older\"\nval pending = {receiver}(p.TEXT + p.TEXT, if (flag) {{ val newer = \"newer\"\nif (flag) {{ return }} else {{ newer == \"newer\" }} }} else {{ true }})\nval used = println(older) }} }}\nvalue class Host(val item: {field}): Relay {{}}"
                    );
                    let consumer = format!(
                        "package q\nimport p.Host\nfun entry(): Unit {{ val host = {constructor}\nval done = host.relay({flag})\nval marker = println(\"done\") }}"
                    );
                    let run = run_counted_strings(
                        &provider,
                        &consumer,
                        5 + extra + receiver_drops,
                        allocations,
                        false,
                    );
                    assert!(
                        run.status.success(),
                        "{receiver}/{field}/{mode}/{flag}: {run:?}"
                    );
                    assert_eq!(run.stdout, expected.as_bytes(), "{run:?}");
                    assert!(run.stderr.is_empty(), "{run:?}");
                }
            }
        }
    }
}

#[test]
fn function_value_prefix_cleanup_preserves_the_closure_and_its_capture() {
    for mode in ["borrow", "own"] {
        for (flag, exit, drops, aborting, expected) in [
            ("true", "return", 5, false, "done\n"),
            ("false", "return", 5, false, "中文\ndone\n"),
            ("true", "error(TEXT)", 2, true, ""),
        ] {
            let provider = format!(
                "package p\nconst val TEXT = \"中文\"\nfun exercise(own flag: Boolean): Unit {{ val captured = TEXT\nval action: move ({mode} String, own Boolean) -> Unit = move {{ text, accepted -> println(captured) }}\nval done = action(TEXT + TEXT, if (flag) {{ {exit} }} else {{ true }}) }}"
            );
            let consumer = format!(
                "package q\nfun entry(): Unit {{ val done = p.exercise({flag})\nval marker = println(\"done\") }}"
            );
            let run = run_counted_strings(&provider, &consumer, drops, 1, aborting);
            assert!(run.status.success(), "{mode}/{flag}/{exit}: {run:?}");
            assert_eq!(run.stdout, expected.as_bytes(), "{run:?}");
            assert!(
                run.stderr.is_empty() || run.stderr.starts_with(b"abort: "),
                "{run:?}"
            );
        }
    }
}

#[test]
fn function_value_loop_exits_match_literal_capture_and_argument_cleanup() {
    for value in ["p.TEXT", "\"中文\""] {
        for mode in ["borrow", "own"] {
            for exit in ["break", "continue"] {
                let provider = "package p\nconst val TEXT = \"中文\"\nconst val FLAG = true";
                let expected_turns = if exit == "continue" { 2 } else { 1 };
                // Both the capture and pending argument own distinct allocated buffers.
                let consumer = format!(
                    "package q\nfun entry(): Unit {{ var turns = 0\nloop {{ turns = turns + 1\nif (turns == 2) {{ break }}\nval captured = {value} + {value}\nval action: move ({mode} String, own Boolean) -> Unit = move {{ text, accepted -> println(captured) }}\nval pending = action({value} + {value}, if (p.FLAG) {{ {exit} }} else {{ true }})\nval unexpected = println(\"unexpected continuation\")\nbreak }}\nif (turns != {expected_turns}) {{ println(\"wrong loop target\") }}\nprintln(\"done\") }}"
                );
                let run = run_counted_strings(provider, &consumer, 7, 2, false);
                assert!(run.status.success(), "{value}/{mode}/{exit}: {run:?}");
                assert_eq!(run.stdout, b"done\n", "{value}/{mode}/{exit}: {run:?}");
                assert!(run.stderr.is_empty(), "{run:?}");
            }
        }
    }
}

#[test]
fn indirect_nothing_argument_break_cleans_up_without_dispatch() {
    let provider = "package p\nconst val TEXT = \"中文\"\nconst val FLAG = true";
    let consumer = r#"
package q
fun entry(): Unit {
    loop {
        val captured = p.TEXT + p.TEXT
        val action: move (borrow String, own Boolean) -> Nothing =
            move { text, accepted -> println(captured); error("unexpected dispatch") }
        val pending = action(p.TEXT + p.TEXT, if (p.FLAG) { break } else { true })
    }
    println("done")
}
"#;
    let run = run_counted_strings(provider, consumer, 7, 2, false);
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"done\n", "{run:?}");
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[test]
fn inner_loops_preserve_pending_string_owner_and_loan() {
    for mode in ["borrow", "own"] {
        for (inner, drops, aborting, expected) in [
            (
                "loop { val seen = println(\"tick\")\nbreak }",
                5,
                false,
                "tick\n中文中文\ndone\n",
            ),
            (
                "var once = true\nloop { val seen = println(\"tick\")\nif (!once) { break }\nonce = false\ncontinue }",
                6,
                false,
                "tick\ntick\n中文中文\ndone\n",
            ),
            (
                "var once = true\nwhile (once) { once = false\nval seen = println(\"tick\") }",
                5,
                false,
                "tick\n中文中文\ndone\n",
            ),
            ("loop { return }", 4, false, "done\n"),
            ("loop { val failed = error(TEXT) }", 2, true, ""),
        ] {
            let provider = format!(
                "package p\nconst val TEXT = \"中文\"\nfun consume({mode} text: String, own flag: Boolean): Unit {{ val done = println(text) }}\nfun exercise(own flag: Boolean): Unit {{ val done = consume(TEXT + TEXT, if (flag) {{ {inner}\ntrue }} else {{ false }}) }}"
            );
            let consumer = "package q\nfun entry(): Unit { val done = p.exercise(true)\nval marker = println(\"done\") }";
            let run = run_counted_strings(&provider, consumer, drops, 1, aborting);
            assert!(run.status.success(), "{mode}/{inner}: {run:?}");
            assert_eq!(run.stdout, expected.as_bytes(), "{run:?}");
            assert!(
                run.stderr.is_empty() || run.stderr.starts_with(b"abort: "),
                "{run:?}"
            );
        }
    }
}

#[test]
fn pending_copyable_argument_is_a_snapshot_across_loop_assignment() {
    let provider = "package p\nconst val TEXT = \"中文\"\nfun consume(text: String, own before: Boolean, own after: Boolean): Unit { if (!before) { val failed = error(TEXT) }\nval seen = println(text) }\nfun exercise(own flag: Boolean): Unit { var once = true\nval done = consume(TEXT + TEXT, once, if (flag) { while (once) { once = false }\ntrue } else { false }) }";
    let consumer = "package q\nfun entry(): Unit { val done = p.exercise(true)\nval marker = println(\"done\") }";
    let run = run_counted_strings(provider, consumer, 4, 1, false);
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, "中文中文\ndone\n".as_bytes(), "{run:?}");
    assert!(run.stderr.is_empty(), "{run:?}");
}

fn run_counted_strings(
    provider_text: &str,
    consumer_text: &str,
    expected_drops: usize,
    expected_allocations: usize,
    aborting: bool,
) -> std::process::Output {
    let mut sources = SourceMap::new();
    let (provider_source, provider) =
        super::super::parsed(&mut sources, "p/provider.ko", provider_text);
    let (consumer_source, consumer) =
        super::super::parsed(&mut sources, "q/consumer.ko", consumer_text);
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let (name_environment, environment) = standard_environments();
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
    let entry = names
        .names()
        .index()
        .declarations()
        .iter()
        .find(|declaration| declaration.name() == "entry")
        .unwrap()
        .id();
    let (program, entry) = crate::ssa::unit_lower::constant::lower_constant_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &environment,
        &typed,
        &owned,
        entry,
    )
    .unwrap();
    let string = program.modules[0]
        .types
        .iter()
        .position(|ty| *ty == crate::ssa::model::SsaTypeKind::StringOwner)
        .unwrap();
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry)
        .unwrap()
        .replace("@malloc(", "@counted_malloc(")
        .replace("@free(", "@counted_free(")
        .replace("@abort(", "@counted_abort(");
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
    assert!(sites > 0, "instrumentation must observe String drop calls");
    instrumented.push_str("declare void @counted_drop()\n");
    let directory = TestDirectory::create();
    let ir = directory.join("owners.ll");
    let counter = directory.join("counter.c");
    let executable = directory.join("owners");
    fs::write(&ir, instrumented).unwrap();
    fs::write(
        &counter,
        format!("#define EXPECT_ABORT {}\n#define EXPECT_DROPS {expected_drops}\n#define EXPECT_ALLOCATIONS {expected_allocations}\n{}", u8::from(aborting), COUNTER),
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
    Command::new(&executable).output().unwrap()
}

const COUNTER: &str = r#"
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
void counted_abort(void) {
    assert(EXPECT_ABORT);
    /* Only concat inputs have dropped; its pending owner and message remain. */
    assert(drops == 2 && allocations == 1 && releases == 0 && live[0]);
    _Exit(0);
}
__attribute__((destructor)) static void verify_counts(void) {
    assert(!EXPECT_ABORT);
    /* Normal completion and early return must release all allocated buffers. */
    assert(drops == EXPECT_DROPS);
    assert(allocations == EXPECT_ALLOCATIONS && releases == EXPECT_ALLOCATIONS && !live[0] && !live[1]);
}
"#;
}

#[test]
fn constant_object_runs_deterministically_and_preserves_output_on_failure() {
    use super::parsed;
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\nconst val TEXT = \"界\"\nconst val FLAG = true",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\nimport p.TEXT\nfun entry(): Unit { println(TEXT + p.TEXT) }\nfun argvEntry(args: Array<String>): Unit { println(TEXT) }\nfun <T> generic(): Unit {}\nfun invalid(): String = TEXT\nfun interpolated(): Unit { println(\"${p.TEXT}\") }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let (name_environment, environment) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
        .unwrap()
        .validate()
        .unwrap();
    let declaration = |name: &str| {
        names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|declaration| declaration.name() == name)
            .unwrap()
            .id()
    };
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &environment)
        .unwrap()
        .validate_constants()
        .unwrap();
    let owned =
        check_compilation_unit_constant_ownership(&sources, &inputs, &names, &environment, &typed)
            .unwrap()
            .validate()
            .unwrap();
    let directory = TestDirectory::create();
    let object = directory.join("program.o");
    fs::write(&object, b"previous object").unwrap();
    for (entry, expected) in [
        (NativeUnitEntry::NoArguments(declaration("entry")), "界界\n"),
        (
            NativeUnitEntry::BorrowedArguments(declaration("argvEntry")),
            "界\n",
        ),
    ] {
        let mut first = None;
        for ordered in [inputs, [inputs[1], inputs[0]], inputs] {
            emit_native_constant_unit_object(
                &sources,
                &ordered,
                &names,
                &environment,
                &typed,
                &owned,
                entry,
                &object,
            )
            .unwrap();
            let bytes = fs::read(&object).unwrap();
            if let Some(first) = &first {
                assert_eq!(&bytes, first);
            } else {
                first = Some(bytes);
            }
            assert_no_sibling_temporary(&directory.0);
        }
        let executable = directory.join("program");
        let linked = Command::new(crate::test_support::clang())
            .arg(&object)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        assert!(linked.status.success(), "{linked:?}");
        let run = Command::new(&executable).arg("参数").output().unwrap();
        assert!(run.status.success(), "{run:?}");
        assert_eq!(run.stdout, expected.as_bytes());
    }
    let original = fs::read(&object).unwrap();
    for (name, expected) in [
        ("generic", NativeObjectErrorKind::InvalidEntry),
        ("invalid", NativeObjectErrorKind::InvalidEntry),
        ("interpolated", NativeObjectErrorKind::UnsupportedSource),
    ] {
        assert_eq!(
            emit_native_constant_unit_object(
                &sources,
                &inputs,
                &names,
                &environment,
                &typed,
                &owned,
                declaration(name),
                &object
            )
            .unwrap_err()
            .kind(),
            expected
        );
        assert_eq!(fs::read(&object).unwrap(), original);
        assert_no_sibling_temporary(&directory.0);
    }
    let foreign = check_compilation_unit_types(&sources, &inputs, &names, &environment)
        .unwrap()
        .validate_constants()
        .unwrap();
    assert_eq!(
        emit_native_constant_unit_object(
            &sources,
            &inputs,
            &names,
            &environment,
            &foreign,
            &owned,
            declaration("entry"),
            &object
        )
        .unwrap_err()
        .kind(),
        NativeObjectErrorKind::MismatchedAnalysis
    );
    assert_eq!(fs::read(&object).unwrap(), original);
    assert_no_sibling_temporary(&directory.0);
}

#[test]
fn all_constant_types_run_across_files_and_namespaces() {
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
        ("Char", "'文'"),
        ("String", "\"中\\0文\""),
    ];
    let declarations = values
        .iter()
        .enumerate()
        .map(|(index, (ty, literal))| format!("const val V{index}: {ty} = {literal}\n"))
        .collect::<String>();
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
        let imports = if prefix.is_empty() {
            (0..values.len())
                .map(|index| format!("import p.V{index}\n"))
                .collect()
        } else {
            "import p.Config\n".to_string()
        };
        let checks = values.iter().enumerate().map(|(index, (ty, literal))| {
            let literal = if *ty == "Char" { "EXPECTED_CHAR" } else { literal };
            format!(
            "val expected{index}: {ty} = {literal}\nif ({prefix}V{index} == expected{index} && p.{prefix}V{index} == expected{index}) {{ println(\"{index}\") }}\n"
        )}).collect::<String>();
        let consumer = format!(
            "package q\n{imports}\nconst val EXPECTED_CHAR: Char = '文'\nconst val OTHER_CHAR: Char = '中'\nconst val CHAIN: Int = p.{prefix}V3 - 2147483605\nfun entry(): Unit {{ {checks}if ({prefix}V9 != OTHER_CHAR && CHAIN == 42) {{ println(\"chain\") }}\nprintln({prefix}V10) }}"
        );
        let run = run_constant_sources(&format!("package p\n{root}"), &consumer);
        assert!(run.status.success(), "{root}: {run:?}");
        assert_eq!(
            run.stdout,
            "0\n1\n2\n3\n4\n5\n6\n7\n8\n9\n10\nchain\n中\0文\n".as_bytes(),
            "{root}"
        );
        assert!(run.stderr.is_empty(), "{run:?}");
    }
}

fn run_constant_sources(provider_text: &str, consumer_text: &str) -> std::process::Output {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = super::parsed(&mut sources, "p/provider.ko", provider_text);
    let (consumer_source, consumer) = super::parsed(&mut sources, "q/consumer.ko", consumer_text);
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let (name_environment, environment) = standard_environments();
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
    let entry = names
        .names()
        .index()
        .declarations()
        .iter()
        .find(|declaration| declaration.name() == "entry")
        .unwrap()
        .id();
    let directory = TestDirectory::create();
    let object = directory.join("matrix.o");
    emit_native_constant_unit_object(
        &sources,
        &inputs,
        &names,
        &environment,
        &typed,
        &owned,
        entry,
        &object,
    )
    .unwrap();
    let executable = directory.join("matrix");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(linked.status.success(), "{linked:?}");
    Command::new(&executable).output().unwrap()
}

#[test]
fn shared_receiver_argument_return_skips_the_native_call() {
    let run = run_constant_sources(
        "package p\nconst val TEXT = \"中文\"\nconst val FLAG = true\nclass Host { fun view(text: String, own flag: Boolean): Unit { println(\"unexpected call\") } }",
        "package q\nimport p.Host\nfun entry(): Unit { val before = println(\"before\")\nval host = Host()\nval done = host.view(p.TEXT, if (p.FLAG) { return } else { true })\nval after = println(\"unexpected continuation\") }",
    );
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"before\n");
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[test]
fn value_receiver_commits_only_after_native_arguments_complete() {
    for (flag, expected) in [("true", "before\n"), ("false", "before\ncall\nafter\n")] {
        let provider = format!(
            "package p\nconst val TEXT = \"中文\"\nconst val FLAG = {flag}\nclass Host {{ own fun consume(text: String, own flag: Boolean): Unit {{ println(\"call\") }} }}"
        );
        let run = run_constant_sources(
            &provider,
            "package q\nimport p.Host\nfun entry(): Unit { val before = println(\"before\")\nval host = Host()\nval done = host.consume(p.TEXT, if (p.FLAG) { return } else { true })\nval after = println(\"after\") }",
        );
        assert!(run.status.success(), "{flag}: {run:?}");
        assert_eq!(run.stdout, expected.as_bytes());
        assert!(run.stderr.is_empty(), "{run:?}");
    }
}

#[test]
fn inout_receiver_preserves_or_writes_back_the_native_value() {
    for (flag, expected) in [("true", "old\n"), ("false", "new\n")] {
        let provider = format!(
            "package p\nconst val TEXT = \"中文\"\nconst val FLAG = {flag}\nvalue class Host(var item: Int) {{ inout fun update(text: String, own flag: Boolean): Unit {{ item = 2 }}\nfun read(): Int = item }}"
        );
        let consumer = "package q\nimport p.Host\nfun entry(): Unit { var host = Host(1)\nloop { val done = host.update(p.TEXT, if (p.FLAG) { break } else { true })\nbreak }\nif (host.read() == 1) { println(\"old\") } else { if (host.read() == 2) { println(\"new\") } else { error(p.TEXT) } } }";
        let run = run_constant_sources(&provider, consumer);
        assert!(run.status.success(), "{flag}: {run:?}");
        assert_eq!(run.stdout, expected.as_bytes());
        assert!(run.stderr.is_empty(), "{run:?}");
    }
}

#[test]
fn receiver_two_phase_native_early_exit_and_normal_activation() {
    for kind in ["class", "value class"] {
        for (flag, expected) in [("true", "old\n"), ("false", "set\nnew\n")] {
            let provider = format!(
                "package p\nconst val TEXT = \"text\"\nconst val FLAG = {flag}\n\
                 {kind} Host(var item: Int) {{\n\
                 fun read(): Int = item\n\
                 inout fun update(text: String, own next: Int): Unit {{ println(\"set\")\nitem = next }}\n}}"
            );
            let consumer = "package q\nimport p.Host\nfun entry(): Unit {\n\
                var host = Host(1)\nloop {\n\
                val done = host.update(p.TEXT + p.TEXT, if (p.FLAG) { break } else { host.read() + 1 })\n\
                break }\n\
                if (host.read() == 1) { println(\"old\") } else {\n\
                if (host.read() == 2) { println(\"new\") } else { error(p.TEXT) } } }";
            let run = run_constant_sources(&provider, consumer);
            assert!(run.status.success(), "{kind} {flag}: {run:?}");
            assert_eq!(run.stdout, expected.as_bytes());
        }
        let provider = format!(
            "package p\nconst val TEXT = \"text\"\n{kind} Host(var item: Int) {{\n\
             fun read(): Int = item\n\
             inout fun update(text: String, own next: Int, own flag: Boolean): Unit {{ println(\"unexpected\") }}\n}}"
        );
        let run = run_constant_sources(
            &provider,
            "package q\nimport p.Host\nfun entry(): Unit { var host = Host(1)\n\
             val done = host.update(p.TEXT + p.TEXT, host.read(), return)\n}",
        );
        assert!(run.status.success(), "{kind}: {run:?}");
        assert!(run.stdout.is_empty(), "{run:?}");
    }
}

mod root_primitive_tests {
//! SPEC-0244：跨文件 root 原语的真实 native 值流、控制前缀与资源回收。
use super::super::analyze_sources;
use super::run_constant_sources;
use crate::ssa::unit_lower::lower_scalar_unit_with_entry;
use crate::test_support::diagnostics::assert_text_eq;

#[test]
fn unit_root_primitive_native_cross_file_order_and_dynamic_owners() {
    let analysis = analyze_sources(
        "package p\nfun replacement(): String { println(\"make\")\nreturn \"new\" + \"!\" }",
        "package q\nfun entry(): Unit {\nvar left = \"left\" + \"!\"\nvar right = \"right\" + \"!\"\nval old = replace(&left, p.replacement())\nswap(&left, &right)\nprintln(old)\nprintln(left)\nprintln(right)\n}",
    );
    let inputs = analysis.inputs();
    let lower = |inputs: &[_]| {
        lower_scalar_unit_with_entry(
            &analysis.sources,
            inputs,
            &analysis.names,
            &analysis.environment,
            &analysis.typed,
            &analysis.owned,
            analysis.declaration("q", "entry"),
        )
        .expect("cross-file ownership primitive SSA")
    };
    let (program, entry) = lower(&inputs);
    let (reversed, reversed_entry) = lower(&[inputs[1], inputs[0]]);
    assert_text_eq!(
        crate::ssa::render_program(&program),
        crate::ssa::render_program(&reversed)
    );
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    assert_eq!(
        llvm,
        crate::llvm::render_verified_program_with_entry(&reversed, reversed_entry).unwrap()
    );
    let output = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 3);
    crate::native_tests::boxed_enum_tests::assert_success(&output, b"make\nleft!\nright!\nnew!\n");
}

#[test]
fn unit_root_primitive_native_control_transfer_preserves_old_root() {
    for exit in ["return", "break", "continue"] {
        let provider = format!(
            "package p\nfun exercise(own flag: Boolean): Unit {{\nvar target = \"old\" + \"!\"\nvar once = true\nloop {{\nif (!once) {{ break }}\nonce = false\nval old = replace(&target, if (flag) {{ {exit} }} else {{ \"new\" + \"!\" }})\nprintln(old)\nbreak\n}}\nprintln(target)\n}}"
        );
        for flag in ["true", "false"] {
            let consumer =
                format!("package q\nfun entry(): Unit {{ p.exercise({flag})\nprintln(\"done\") }}");
            let run = run_constant_sources(&provider, &consumer);
            assert!(run.status.success(), "{exit}/{flag}: {run:?}");
            let expected = if flag == "false" {
                "old!\nnew!\ndone\n"
            } else if exit == "return" {
                "done\n"
            } else {
                "old!\ndone\n"
            };
            assert_eq!(run.stdout, expected.as_bytes(), "{exit}/{flag}: {run:?}");
            assert!(run.stderr.is_empty(), "{exit}/{flag}: {run:?}");
        }
    }
}

#[test]
fn unit_root_primitive_native_copyable_aliases_and_unit_roots() {
    let run = run_constant_sources(
        "package p\nfun make(): Unit {}",
        "package q\nfun entry(): Unit {\nvar left = 7\nvar right = left\nval source = right\nval old = replace(&left, source)\nswap(&left, &right)\nif (old + left + right != 21) { error(\"bad scalar exchange\") }\nvar first = p.make()\nvar second = p.make()\nval previous = replace(&first, p.make())\nswap(&first, &second)\nprintln(\"done\")\n}",
    );
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"done\n");
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[test]
fn unit_root_primitive_native_abort_never_reaches_commit_continuation() {
    let run = run_constant_sources(
        "package p\nconst val MESSAGE = \"stop\"",
        "package q\nfun entry(): Unit {\nvar target = \"old\" + \"!\"\nval unused = replace(&target, error(p.MESSAGE))\nprintln(\"unexpected continuation\")\n}",
    );
    assert!(!run.status.success(), "Nothing must abort: {run:?}");
    assert!(run.stdout.is_empty(), "{run:?}");
}

#[test]
fn unit_root_primitive_native_nested_copyable_and_unit_loop_prefixes() {
    let provider = "package p\nfun make(): Unit {}\nfun scalar(own flag: Boolean): Int {\nvar target = 7\nval old = replace(&target, if (flag) { loop { break }\n9 } else { 11 })\nreturn old + target\n}\nfun unitRoot(own flag: Boolean): Unit {\nvar target = make()\nloop {\nval old = replace(&target, if (flag) { break } else { make() })\nbreak\n}\ntarget\nprintln(\"unit\")\n}";
    for (flag, result) in [("true", 16), ("false", 18)] {
        let consumer = format!(
            "package q\nfun entry(): Unit {{\nif (p.scalar({flag}) != {result}) {{ error(\"bad pending root\") }}\np.unitRoot({flag})\nprintln(\"done\")\n}}"
        );
        let run = run_constant_sources(provider, &consumer);
        assert!(run.status.success(), "{flag}: {run:?}");
        assert_eq!(run.stdout, b"unit\ndone\n", "{flag}: {run:?}");
        assert!(run.stderr.is_empty(), "{flag}: {run:?}");
    }
}
}

#[test]
fn constant_never_rhs_keeps_the_only_reachable_native_branch() {
    let run = run_constant_sources(
        "package p
const val TRUE = true
const val FALSE = false",
        "package q
         fun orCase(flag: Boolean): Int {
             when { flag -> {}; else -> {} }
             if (p.TRUE || ((return 7) == 0)) { return 8 }
         }
         fun andCase(flag: Boolean): Int {
             when { flag -> {}; else -> {} }
             if (p.FALSE && ((return 7) == 0)) {} else { return 9 }
         }
         fun entry(): Unit {
             if (orCase(false) != 8 || andCase(true) != 9) { error(\"wrong branch\") }
             println(\"ok\")
         }",
    );
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"ok\n", "{run:?}");
    assert!(run.stderr.is_empty(), "{run:?}");
}
