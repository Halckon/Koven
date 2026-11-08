//! A reachable heap If proof may not depend on an unrelated nullable When.
use super::{SymbolKind, UnitSource};

const CONTROL: &str = r#"class Node(val v: Int)
fun observe(node: Node): Unit {
    if (node.v == 7) { println("present") }
}
fun scan(own node: Node?): Unit {
    var text: String? = "first"
    if (text != null) {
        var i = 0
        while (i < 2) {
            text = "second"
            break
        }
    }
    if (node != null) { observe(node) } else {}
}
public fun main(): Unit {
    val node: Node? = Node(7)
    scan(node)
}
"#;

const SIBLING: &str = r#"fun unrelated(own node: Node?): Unit {
    when (node) {
        null -> println("absent")
        else -> println("present")
    }
}
"#;

#[test]
fn heap_nullable_if_does_not_depend_on_an_uncalled_nullable_when() {
    for (path, sibling) in [("control.ko", ""), ("sibling.ko", SIBLING)] {
        let text = format!("{CONTROL}{sibling}");
        let source = UnitSource::analyze(path, &text).expect("strict frontend admission");
        source.assert_frontend_clean();
        let entry = source.declaration("main", SymbolKind::Function);
        source.lower_entry(entry).unwrap_or_else(|error| {
            let text = error
                .span
                .and_then(|span| source.sources().slice(span).ok());
            panic!("{path}: {error:?} {text:?}")
        });
        let run = crate::native_tests::emit_link_and_run(path, &text, "main");
        crate::native_tests::boxed_enum_tests::assert_success(&run, b"present\n");
    }
}

#[test]
fn heap_nullable_if_stays_retired_on_forced_per_family_lowering() {
    let text = format!(
        r#"class Node(val v: Int)
fun observe(node: Node): Unit {{ println("present") }}
fun scan(own node: Node?): Unit {{
    if (node != null) {{ observe(node) }} else {{}}
}}
public fun main(): Unit {{ scan(Node(7)) }}
{SIBLING}"#
    );
    let source = UnitSource::analyze("refined-if.ko", &text).expect("strict frontend admission");
    source.assert_frontend_clean();
    let entry = source.declaration("main", SymbolKind::Function);
    source
        .lower_entry(entry)
        .expect("default refinement lowers");
    let error = crate::ssa::with_per_family_lowering(|| source.lower_entry(entry))
        .err()
        .expect("forcing per-family cannot restore the retired If route");
    assert!(matches!(
        error.kind,
        crate::ssa::LoweringErrorKind::UnsupportedNode | crate::ssa::LoweringErrorKind::MissingFact
    ));
    assert_eq!(
        source.sources().slice(error.span.expect("source span")),
        Ok("node")
    );
}
