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
    let run = run_counted_strings(
        "package p\nconst val TEXT = \"中文\"\nconst val FLAG = true\nfun view_pair(text: String, own flag: Boolean): Unit {}",
        "package q\nfun entry(): Unit { val pending = p.view_pair(p.TEXT + p.TEXT, if (p.FLAG) { error(p.TEXT) } else { true }) }",
        2,
        1,
        true,
    );
    assert!(
        run.status.success(),
        "counted Abort must check state before exiting: {run:?}"
    );
    assert!(run.stdout.is_empty() && run.stderr.is_empty(), "{run:?}");
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
            assert!(run.stderr.is_empty(), "{run:?}");
        }
    }
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
    let linked = Command::new("/usr/bin/clang")
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
