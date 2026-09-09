use super::emit_link_and_run;
use std::os::unix::process::ExitStatusExt;

#[test]
fn non_null_assertion_pointer_results_and_shadowed_error_run_natively() {
    for (inner, constructor, payload) in [
        ("Node", "Node(37)", "node.item"),
        ("Box<Token>", "Box(Token(37))", ""),
        ("Rc<Int>", "Rc(37)", "node.value"),
    ] {
        let observe = if payload.is_empty() {
            // Box payload projections remain deferred; accepting its Value parameter tests delivery.
            "println(\"ok\")".to_owned()
        } else {
            format!("if ({payload} == 37) {{ println(\"ok\") }} else {{ println(\"bad\") }}")
        };
        let declarations = format!(
            r#"
class Node(val item: Int)
value class Token(val item: Int)
fun error(message: String): Int {{ val printed = println("shadow")
return 7 }}
fun make(): {inner}? {{ val printed = println("make")
val result: {inner}? = {constructor}
return result }}
fun observe(own node: {inner}): Unit {{ {observe} }}
"#
        );
        let source = format!(
            r#"{declarations}
fun entry(): Unit {{ val explicit = error("ordinary")
val checked = if (explicit == 7) {{ observe(make()!!) }} else {{ println("wrong target") }} }}
"#
        );
        let run = emit_link_and_run("assertion-present.ko", &source, "entry");
        assert!(run.status.success(), "{inner}: {run:?}");
        assert_eq!(run.stdout, b"shadow\nmake\nok\n", "{inner}");
        assert!(run.stderr.is_empty(), "{inner}: {run:?}");
        let absent = format!(
            r#"{declarations}
fun entry(): Unit {{ val node: {inner}? = null
val checked = observe(node!!) }}
"#
        );
        let run = emit_link_and_run("assertion-null.ko", &absent, "entry");
        assert_eq!(
            run.status.signal(),
            Some(6),
            "{inner}: assertion must SIGABRT despite shadowed error: {run:?}"
        );
    }
}

#[test]
fn non_null_assertion_transfers_one_allocation_and_preserves_explicit_rc_alias() {
    use super::{TestDirectory, analyze, symbol};
    use lang_frontend::name_resolution::SymbolKind;
    use std::{fs, process::Command};

    for (inner, constructor, payload) in [
        ("Node", "Node(37)", "node.item"),
        ("Box<Token>", "Box(Token(37))", ""),
        ("Rc<Int>", "Rc(37)", "node.value"),
    ] {
        let shared = usize::from(inner == "Rc<Int>");
        let keep = if shared == 1 {
            "val alias = node.share()\nreturn alias"
        } else {
            "return node"
        };
        let observe = if payload.is_empty() {
            "println(\"ok\")".to_owned()
        } else {
            format!("if ({payload} == 37) {{ println(\"ok\") }} else {{ println(\"bad\") }}")
        };
        let source = format!(
            r#"
class Node(val item: Int)
value class Token(val item: Int)
fun make(): {inner}? {{ val printed = println("make")
val result: {inner}? = {constructor}
return result }}
fun keep(own node: {inner}): {inner} {{ {keep} }}
fun observe(own node: {inner}): Unit {{ {observe} }}
fun entry(): Unit {{ val checked = observe(keep(make()!!)) }}
"#
        );
        let analysis = analyze("assertion-counted.ko", &source);
        let (program, entry) = crate::ssa::lower_scalar_file_with_entry(
            &analysis.sources,
            &analysis.parsed,
            &analysis.names,
            &analysis.typed,
            &analysis.owned,
            symbol(&analysis, "entry", SymbolKind::Function),
        )
        .expect("counted assertion source must verify");
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry)
            .expect("counted assertion LLVM must verify")
            .replace("@malloc(", "@counted_malloc(")
            .replace("@free(", "@counted_free(");
        let mut instrumented = String::new();
        let mut retain_sites = 0;
        let mut release_sites = 0;
        for line in llvm.lines() {
            instrumented.push_str(line);
            instrumented.push('\n');
            if line.contains(".next = add i64") {
                retain_sites += 1;
                instrumented.push_str("  call void @counted_retain()\n");
            } else if line.contains("%strong.next = sub i64") {
                release_sites += 1;
                instrumented.push_str("  call void @counted_release()\n");
            }
        }
        assert_eq!((retain_sites, release_sites), (shared, shared));
        instrumented.push_str("declare void @counted_retain()\ndeclare void @counted_release()\n");
        let directory = TestDirectory::create();
        let ir = directory.join("assertion.ll");
        let counter = directory.join("counter.c");
        let executable = directory.join("counted-assertion");
        fs::write(&ir, instrumented).expect("write instrumented LLVM");
        fs::write(
            &counter,
            r#"
#include <stdlib.h>
#include <assert.h>
static void *live;
static int allocations, releases, retains, shared_releases;
void counted_retain(void) { ++retains; }
void counted_release(void) { ++shared_releases; }
void *counted_malloc(size_t size) {
    assert(allocations++ == 0);
    live = malloc(size);
    assert(live);
    return live;
}
void counted_free(void *value) {
    assert(value == live && releases++ == 0);
    free(value);
    live = 0;
}
__attribute__((destructor)) static void verify_counts(void) {
    assert(allocations == 1 && releases == 1 && !live);
    assert(retains == EXPECT_SHARED);
    assert(shared_releases == 2 * EXPECT_SHARED);
}
"#,
        )
        .expect("write dynamic ownership counter");
        let linked = Command::new("/usr/bin/clang")
            .arg(&ir)
            .arg(&counter)
            .arg(format!("-DEXPECT_SHARED={shared}"))
            .arg("-o")
            .arg(&executable)
            .output()
            .expect("clang must launch");
        assert!(linked.status.success(), "{inner}: {linked:?}");
        let run = Command::new(&executable)
            .output()
            .expect("counted program must launch");
        assert!(run.status.success(), "{inner}: {run:?}");
        assert_eq!(run.stdout, b"make\nok\n", "{inner}");
        assert!(run.stderr.is_empty(), "{inner}: {run:?}");
    }
}
