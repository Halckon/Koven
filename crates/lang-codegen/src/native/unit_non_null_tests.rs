use super::{Command, TestDirectory, analyze_sources, emit_native_unit_object};
use std::os::unix::process::ExitStatusExt;

#[test]
fn unit_non_null_assertion_cross_file_results_and_shadowed_error_run_natively() {
    for (inner, constructor, payload) in [
        ("Node", "Node(37)", "node.item"),
        ("Box<Token>", "Box(Token(37))", ""),
        ("Rc<Int>", "Rc(37)", "node.value"),
    ] {
        let observe = if payload.is_empty() {
            // Box payload projection is deferred; its Value parameter verifies owner delivery.
            "println(\"ok\")".to_owned()
        } else {
            format!("if ({payload} == 37) {{ println(\"ok\") }} else {{ println(\"bad\") }}")
        };
        let provider = format!(
            r#"package p
class Node(val item: Int)
value class Token(val item: Int)
fun make(present: Boolean): {inner}? {{ val printed = println("make")
if (present) {{ val result: {inner}? = {constructor}
return result }}
val absent: {inner}? = null
return absent }}
fun observe(own node: {inner}): Unit {{ {observe} }}
"#
        );
        for present in [true, false] {
            let consumer = format!(
                r#"package q
fun error(message: String): Int {{ val printed = println("shadow")
return 7 }}
fun entry(): Unit {{ val explicit = error("ordinary")
val checked = if (explicit == 7) {{ p.observe(p.make({present})!!) }} else {{ println("wrong target") }} }}
"#
            );
            let analysis = analyze_sources(&provider, &consumer);
            let directory = TestDirectory::create();
            let object = directory.join("assertion.o");
            let executable = directory.join("assertion");
            emit_native_unit_object(
                &analysis.sources,
                &analysis.inputs(),
                &analysis.names,
                &analysis.environment,
                &analysis.typed,
                &analysis.owned,
                analysis.declaration("q", "entry"),
                &object,
            )
            .expect("cross-file assertion must emit a verified object");
            let linked = Command::new("/usr/bin/clang")
                .arg(&object)
                .arg("-o")
                .arg(&executable)
                .output()
                .expect("clang must launch");
            assert!(linked.status.success(), "{inner}/{present}: {linked:?}");
            let run = Command::new(&executable)
                .output()
                .expect("assertion executable must launch");
            if present {
                assert!(run.status.success(), "{inner}: {run:?}");
                assert_eq!(run.stdout, b"shadow\nmake\nok\n", "{inner}");
                assert!(run.stderr.is_empty(), "{inner}: {run:?}");
            } else {
                assert_eq!(run.status.signal(), Some(6), "{inner}: {run:?}");
            }
        }
    }
}

#[test]
fn unit_non_null_assertion_pending_temporary_is_freed_after_control_flow_call() {
    let mut consumer = String::from("package q\n");
    let operands = [
        "if (flag) { p.make() } else { p.make() }",
        "when (flag) { true -> p.make(); false -> p.make() }",
        "when { flag -> p.make(); else -> p.make() }",
        "when (flag) { true, false -> p.make() }",
        "when { flag, flag -> p.make(); else -> p.make() }",
        "if (1 + 2 == 3) { p.make() } else { p.make() }",
        "if (flag && flag) { p.make() } else { p.make() }",
    ];
    for (index, operand) in operands.iter().enumerate() {
        consumer.push_str(&format!(
            "fun inspect{index}(flag: Boolean): Int = p.take(Rc(11), ({operand})!!)\n"
        ));
    }
    consumer.push_str("fun entry(): Unit {\n");
    for index in 0..operands.len() {
        for flag in [true, false] {
            consumer.push_str(&format!(
                "val checked{index}{flag} = if (inspect{index}({flag}) == 32) {{ println(\"ok\") }} else {{ println(\"bad\") }}\n"
            ));
        }
    }
    consumer.push_str("}\n");
    let analysis = analyze_sources(
        "package p\nfun make(): Rc<Int>? = Rc(32)\nfun take(first: Rc<Int>, own second: Rc<Int>): Int = second.value",
        &consumer,
    );
    let (program, entry) = super::lower_scalar_unit_with_entry(
        &analysis.sources,
        &analysis.inputs(),
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        analysis.declaration("q", "entry"),
    )
    .expect("pending temporary assertions must verify");
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry)
        .expect("pending temporary LLVM must verify")
        .replace("@malloc(", "@counted_malloc(")
        .replace("@free(", "@counted_free(");
    let directory = TestDirectory::create();
    let ir = directory.join("assertion.ll");
    let counter = directory.join("counter.c");
    let executable = directory.join("assertion-counted");
    std::fs::write(&ir, llvm).expect("write counted LLVM");
    // Track live identities too: a duplicate free cannot compensate for a leaked owner.
    std::fs::write(
        &counter,
        r#"
#include <stdlib.h>
#include <assert.h>
static void *live[28];
static int allocations, releases;
void *counted_malloc(size_t size) {
    assert(allocations < 28);
    void *value = malloc(size);
    assert(value);
    live[allocations++] = value;
    return value;
}
void counted_free(void *value) {
    for (int i = 0; i < allocations; ++i) {
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
    assert(allocations == 28 && releases == 28);
    for (int i = 0; i < allocations; ++i) assert(!live[i]);
}
"#,
    )
    .expect("write ownership counter");
    let linked = Command::new("/usr/bin/clang")
        .arg(&ir)
        .arg(&counter)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable)
        .output()
        .expect("counted executable must launch");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, "ok\n".repeat(14).as_bytes());
    assert!(run.stderr.is_empty(), "{run:?}");
}
