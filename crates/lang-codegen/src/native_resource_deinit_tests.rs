use super::emit_link_and_run;

#[test]
fn resource_deinit_native_scope_body_and_reverse_fields() {
    let output = emit_link_and_run(
        "resource-scope.ko",
        r#"
class Leaf(val name: String) {
    deinit() { println(this.name) }
}
class Guard(val first: Leaf, val second: Leaf) {
    deinit() { println("parent alive") }
}
fun main(): Unit {
    val earlier = Leaf("earlier")
    val guard = Guard(Leaf("first"), Leaf("second"))
    println("body 完成")
}
"#,
        "main",
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        output.stdout,
        "body 完成\nparent alive\nsecond\nfirst\nearlier\n".as_bytes()
    );
}

const LEAF: &str = r#"
class Leaf(val name: String) { deinit() { println(this.name) } }
fun inspect(item: Leaf): Unit { println("inspect") }
fun consume(own item: Leaf): Unit { println("consume") }
"#;

#[test]
fn resource_deinit_native_transfer_return_and_temporary_are_exactly_once() {
    let source = format!(
        r#"{LEAF}
fun make(): Leaf {{ val local = Leaf("returned"); return local }}
fun branch(flag: Boolean): Unit {{
    val outer = Leaf("outer")
    if (flag) {{ val inner = Leaf("inner"); println("return"); return }}
    println("fallthrough")
}}
fun main(): Unit {{
    val source = make()
    val destination = source
    println("before")
    consume(destination)
    inspect(Leaf("temporary"))
    branch(true)
    branch(false)
    println("done")
}}
"#
    );
    let output = emit_link_and_run("resource-transfer.ko", &source, "main");
    assert!(output.status.success(), "{output:?}");
    assert_eq!(output.stdout, b"before\nconsume\nreturned\ninspect\ntemporary\nreturn\ninner\nouter\nfallthrough\nouter\ndone\n");
}

#[test]
fn resource_deinit_native_continue_break_and_nested_scope_preserve_outer_guard() {
    let source = format!(
        r#"{LEAF}
fun main(): Unit {{
    val outer = Leaf("outer")
    var i: Int = 0
    while (i < 3) {{
        val inner = Leaf("inner")
        i = i + 1
        if (i == 1) {{ println("continue"); continue }}
        if (i == 2) {{ println("break"); break }}
    }}
    println("after loop")
}}
"#
    );
    let output = emit_link_and_run("resource-loop.ko", &source, "main");
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        output.stdout,
        b"continue\ninner\nbreak\ninner\nafter loop\nouter\n"
    );
}

#[test]
fn resource_deinit_native_body_return_still_cleans_fields_but_abort_does_not() {
    let source = r#"
class Leaf { deinit() { println("field") } }
class Returning(val child: Leaf) { deinit() { println("body"); return } }
fun main(): Unit { val resource = Returning(Leaf()); println("scope") }
"#;
    let output = emit_link_and_run("resource-return.ko", source, "main");
    assert!(output.status.success(), "{output:?}");
    assert_eq!(output.stdout, b"scope\nbody\nfield\n");
    let source = r#"
class Leaf { deinit() { println("field must not run") } }
class Aborting(val child: Leaf) { deinit() { println("body abort"); error("stop") } }
fun main(): Unit { val resource = Aborting(Leaf()); println("scope") }
"#;
    let output = emit_link_and_run("resource-abort.ko", source, "main");
    use std::os::unix::process::ExitStatusExt;
    assert_eq!(output.status.signal(), Some(6), "{output:?}");
    assert_eq!(output.stdout, b"scope\nbody abort\n");
}

#[test]
fn resource_deinit_native_nested_resource_allocations_have_unique_frees() {
    use super::{SymbolKind, analyze, symbol};
    let source = r#"
class Leaf(val name: String) { deinit() { println(this.name) } }
class Bundle(val first: Leaf, val second: Leaf) { deinit() { println("bundle") } }
class Wrapper(val bundle: Bundle)
fun entry(): Unit {
    val wrapper = Wrapper(Bundle(Leaf("a" + "1"), Leaf("b" + "2")))
    println("held")
}
"#;
    let analysis = analyze("resource-counts.ko", source);
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
    .unwrap();
    let ir = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let output = super::boxed_enum_tests::run_counted_allocations(&ir, 6);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(output.stdout, b"held\nbundle\nb2\na1\n");
}

#[test]
fn resource_deinit_native_replacement_is_after_rhs_and_keeps_declaration_order() {
    let source = format!(
        r#"{LEAF}
fun make(): Leaf {{ println("new rhs"); return Leaf("new") }}
fun main(): Unit {{
    var first = Leaf("old")
    val second = Leaf("second")
    first = make()
    println("after replacement")
}}
"#
    );
    let output = emit_link_and_run("resource-replace.ko", &source, "main");
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        output.stdout,
        b"new rhs\nold\nafter replacement\nsecond\nnew\n"
    );
}

#[test]
fn resource_deinit_native_conditional_owner_is_rejected_without_overwriting_output() {
    use super::{SymbolKind, TestDirectory, analyze, emit_native_object, symbol};
    let source = format!(
        r#"{LEAF}
fun run(flag: Boolean): Unit {{
    val resource = Leaf("resource")
    if (flag) {{ consume(resource) }}
    println("after branch")
}}
fun main(): Unit {{ run(true); run(false) }}
"#
    );
    let analysis = analyze("resource-conditional.ko", &source);
    assert!(
        analysis.owned.diagnostics().is_empty(),
        "{:?}",
        analysis.owned.diagnostics()
    );
    let directory = TestDirectory::create();
    let path = directory.join("preserved.o");
    std::fs::write(&path, b"previous object").unwrap();
    let error = emit_native_object(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
        symbol(&analysis, "main", SymbolKind::Function),
        &path,
    )
    .expect_err("conditional owner transport remains outside this bounded native slice");
    assert_eq!(
        error.kind(),
        super::NativeObjectErrorKind::UnsupportedSource
    );
    let span = error
        .span()
        .expect("rejection keeps the conditional owner origin");
    assert_eq!(
        &source[span.start() as usize..span.end() as usize],
        "resource"
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"previous object");
}
