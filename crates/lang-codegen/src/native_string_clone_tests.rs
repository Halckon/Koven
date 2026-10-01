use super::emit_link_and_run;

#[test]
fn string_clone_preserves_utf8_nul_empty_and_borrowed_source() {
    let output = emit_link_and_run(
        "string-clone.ko",
        r#"
        fun duplicate(text: String): String = text.clone()
        fun stringCloneEntry(): Unit {
            val source = "界\0" + "é"
            val copy = duplicate(source)
            println(source)
            println(copy)
            println("literal".clone())
            println("".clone())
        }
        "#,
        "stringCloneEntry",
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(output.stdout, "界\0é\n界\0é\nliteral\n\n".as_bytes());
}

#[test]
fn string_clone_copies_owned_container_elements_without_moving_them() {
    let output = emit_link_and_run(
        "string-clone-elements.ko",
        r#"
        fun stringCloneEntry(): Unit {
            val texts = listOf("left" + "right", "界")
            val copy = texts[0].clone()
            println(texts[0])
            println(copy)
            println(texts[1].clone())
        }
        "#,
        "stringCloneEntry",
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(output.stdout, "leftright\nleftright\n界\n".as_bytes());
}

#[test]
fn string_clone_heap_owners_free_once_and_static_empty_clone_has_exact_allocation_cost() {
    use super::{TestDirectory, analyze, symbol};
    use lang_frontend::name_resolution::SymbolKind;
    use std::{fs, process::Command};
    let source = r#"
        fun duplicate(text: String): String = text.clone()
        fun consume(own text: String): Unit { println(text) }
        fun stringCloneEntry(): Unit {
            val source = "界\0" + "é"
            val first = duplicate(source)
            consume(first)
            println(source)
            val second = source.clone()
            println(second)
            consume("static".clone())
            consume("".clone())
        }
    "#;
    let analysis = analyze("clone-counted.ko", source);
    let (program, entry) = crate::ssa::lower_scalar_file_with_entry(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
        symbol(&analysis, "stringCloneEntry", SymbolKind::Function),
    )
    .expect("clone owner lifecycle verifies");
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry)
        .unwrap()
        .replace("@malloc(", "@counted_malloc(")
        .replace("@free(", "@counted_free(");
    let directory = TestDirectory::create();
    let ir = directory.join("clone.ll");
    let counter = directory.join("counter.c");
    let executable = directory.join("clone");
    fs::write(&ir, llvm).unwrap();
    fs::write(
        &counter,
        r#"
#include <stdlib.h>
#include <assert.h>
static void *live[4];
static int allocations, releases;
void *counted_malloc(size_t size) {
    assert(size > 0 && allocations < 4);
    void *pointer = malloc(size);
    assert(pointer);
    for (int i = 0; i < allocations; ++i) assert(live[i] != pointer);
    live[allocations++] = pointer;
    return pointer;
}
void counted_free(void *pointer) {
    assert(pointer);
    for (int i = 0; i < allocations; ++i) {
        if (live[i] == pointer) {
            live[i] = 0;
            ++releases;
            free(pointer);
            return;
        }
    }
    abort();
}
__attribute__((destructor)) static void verify_counts(void) {
    assert(allocations == 4 && releases == 4);
    for (int i = 0; i < allocations; ++i) assert(!live[i]);
}
"#,
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
    assert_eq!(run.stdout, "界\0é\n界\0é\n界\0é\nstatic\n\n".as_bytes());
}

#[test]
fn string_clone_evaluates_temporary_receiver_once_and_survives_source_return_drop() {
    let output = emit_link_and_run(
        "clone-temporary.ko",
        r#"
        fun make(): String { println("made") return "dynamic" + "source" }
        fun duplicateAndDrop(own source: String): String = source.clone()
        fun element(texts: List<String>): String = texts[0].clone()
        fun stringCloneEntry(): Unit {
            val copy = make().clone()
            println(copy)
            val returned = duplicateAndDrop("前" + "后")
            println(returned)
            val texts = listOf("元素" + "值")
            println(element(texts))
            println(texts[0])
            println(("" + "").clone())
            println("nested".clone().clone())
        }
        "#,
        "stringCloneEntry",
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        output.stdout,
        "made\ndynamicsource\n前后\n元素值\n元素值\n\nnested\n".as_bytes()
    );
}

#[test]
fn string_clone_allocation_failure_aborts_before_publishing_result() {
    use super::{TestDirectory, analyze, symbol};
    use lang_frontend::name_resolution::SymbolKind;
    use std::{fs, process::Command};
    let analysis = analyze(
        "clone-failure.ko",
        r#"fun stringCloneEntry(): Unit { println("copy".clone()) }"#,
    );
    let (program, entry) = crate::ssa::lower_scalar_file_with_entry(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
        symbol(&analysis, "stringCloneEntry", SymbolKind::Function),
    )
    .unwrap();
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry)
        .unwrap()
        .replace("@malloc(", "@failing_malloc(")
        .replace("@abort(", "@expected_abort(");
    let directory = TestDirectory::create();
    let ir = directory.join("failure.ll");
    let counter = directory.join("failure.c");
    let executable = directory.join("failure");
    fs::write(&ir, llvm).unwrap();
    fs::write(
        &counter,
        r#"
#include <stdlib.h>
#include <assert.h>
static int allocations;
void *failing_malloc(size_t size) { assert(size == 4); ++allocations; return 0; }
void expected_abort(void) { assert(allocations == 1); _Exit(0); }
__attribute__((destructor)) static void unexpected_return(void) { abort(); }
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
    assert!(run.stdout.is_empty() && run.stderr.is_empty(), "{run:?}");
}

#[test]
fn string_clone_borrows_temporary_container_element() {
    let output = emit_link_and_run(
        "clone-temporary-element.ko",
        r#"
        fun stringCloneEntry(): Unit { println(listOf("元" + "素")[0].clone()) }
    "#,
        "stringCloneEntry",
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(output.stdout, "元素\n".as_bytes());
}

#[test]
fn string_clone_borrows_named_fields_and_rc_payloads() {
    let output = emit_link_and_run(
        "clone-field.ko",
        r#"
        class Holder(val text: String)
        value class Value(val text: String)
        fun fieldCopy(holder: Holder): String = holder.text.clone()
        fun stringCloneEntry(): Unit {
            val holder = Holder("field" + "value")
            println(holder.text.clone())
            println(fieldCopy(holder))
            val value = Value("inline")
            println(value.text.clone())
            val shared = Rc("shared" + "value")
            println(shared.value.clone())
        }
    "#,
        "stringCloneEntry",
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        output.stdout,
        b"fieldvalue\nfieldvalue\ninline\nsharedvalue\n"
    );
}

#[test]
fn string_clone_borrowed_rc_payload_is_rejected_at_existing_native_abi_boundary() {
    use super::{analyze, symbol};
    use lang_frontend::name_resolution::SymbolKind;
    let analysis = analyze(
        "clone-borrowed-rc.ko",
        r#"
        fun duplicate(shared: Rc<String>): String = shared.value.clone()
        fun stringCloneEntry(): Unit { val shared = Rc("text"); println(duplicate(shared)) }
    "#,
    );
    assert!(analysis.typed.diagnostics().is_empty());
    assert!(analysis.owned.diagnostics().is_empty());
    let error = crate::ssa::lower_scalar_file_with_entry(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
        symbol(&analysis, "stringCloneEntry", SymbolKind::Function),
    )
    .err()
    .expect("Borrow Rc payload has no normalized native loan ABI yet");
    assert_eq!(error.kind, crate::ssa::LoweringErrorKind::UnsupportedNode);
}

#[test]
fn string_clone_owned_result_supports_move_capture_and_early_return() {
    let output = emit_link_and_run(
        "clone-capture.ko",
        r#"
        fun capture(source: String, early: Boolean): Unit {
            val copy = source.clone()
            val action = move { println(copy) }
            if (early) return
            val called = action()
        }
        fun stringCloneEntry(): Unit {
            val source = "captured" + "copy"
            capture(source, true)
            capture(source, false)
            println(source)
        }
    "#,
        "stringCloneEntry",
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(output.stdout, b"capturedcopy\ncapturedcopy\n");
}

#[test]
fn integrated_numeric_element_clone_moves_into_boxed_enum_and_drops_once() {
    use super::{analyze, symbol};
    use lang_frontend::name_resolution::SymbolKind;
    let analysis = analyze(
        "integrated-clone-box.ko",
        r#"
        enum class Text { Value(text: String), Empty }
        fun consume(own value: Box<Text>): Unit { println("boxed") }
        fun entry(): Unit {
            val texts = listOf("unused", "界" + "é")
            val copy = texts[0x0_1].clone()
            val value = Text.Value(copy)
            consume(Box(value))
            println(texts[0b0_1])
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
    .expect("numeric element clone and boxed enum ownership compose");
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let output = super::boxed_enum_tests::run_counted_allocations(&llvm, 4);
    super::boxed_enum_tests::assert_success(&output, "boxed\n界é\n".as_bytes());
}
