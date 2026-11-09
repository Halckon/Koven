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

#[test]
fn heap_nullable_if_composes_with_borrowed_nullable_presence() {
    let text = r#"class Node(val v: Int)
fun observe(node: Node): Unit {
    if (node.v == 7) { println("present") }
}
fun gate(): Boolean { println("rhs"); return true }
fun scan(input: String?, own node: Node?): Unit {
    if (input != null && gate()) { println("borrow-present") } else { println("borrow-absent") }
    if (node != null) { observe(node) } else {}
}
public fun main(): Unit {
    val present: String? = ""
    scan(present, Node(7))
    scan(null, Node(7))
}
"#;
    let path = "heap-if-with-borrowed-presence.ko";
    let source = UnitSource::analyze(path, text).expect("strict frontend admission");
    source.assert_frontend_clean();
    let entry = source.declaration("main", SymbolKind::Function);
    let (program, _) = source.lower_entry(entry).unwrap_or_else(|error| {
        let text = error
            .span
            .and_then(|span| source.sources().slice(span).ok());
        panic!("{error:?} {text:?}")
    });
    let scan_span = source.declaration_span(source.declaration("scan", SymbolKind::Function));
    let scan = program
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| function.origin.span() == scan_span)
        .expect("source scan function");
    let [super::EntityId::Loan(input), super::EntityId::Value(_)] =
        scan.blocks[0].parameters.as_slice()
    else {
        panic!("scan has a borrowed input and an owned nullable Node");
    };
    assert!(scan.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        super::Operation::NullableLoanIsNull { source } if source == *input
    )));
    let run = crate::native_tests::emit_link_and_run(path, text, "main");
    crate::native_tests::boxed_enum_tests::assert_success(
        &run,
        b"rhs\nborrow-present\npresent\nborrow-absent\npresent\n",
    );
}

#[test]
fn heap_nullable_if_borrowed_presence_uses_the_reached_map_result_layout() {
    let text = r#"class Node(val v: Int)
fun observe(node: Node): Unit {
    if (node.v == 7) { println("present") }
}
fun scan(input: String?, own node: Node?): Unit {
    if (input != null) { println("borrow-present") } else { println("borrow-absent") }
    if (node != null) { observe(node) } else {}
}
public fun main(): Unit {
    var values = mutableMapOf<Int, String>()
    values.put(1, "input")
    val removed = values.remove(1)
    scan(removed, Node(7))
    scan(null, Node(7))
}
"#;
    let compatible = text.replace(
        "if (node != null) { observe(node) } else {}",
        "when (node) { null -> println(\"node-absent\")\nelse -> println(\"present\") }",
    );
    for (path, text) in [
        ("heap-if-with-borrowed-map-result.ko", text),
        (
            "nullable-when-with-borrowed-map-result.ko",
            compatible.as_str(),
        ),
    ] {
        let source = UnitSource::analyze(path, text).expect("strict frontend admission");
        source.assert_frontend_clean();
        let entry = source.declaration("main", SymbolKind::Function);
        source.lower_entry(entry).unwrap_or_else(|error| {
            let text = error
                .span
                .and_then(|span| source.sources().slice(span).ok());
            panic!("{path}: {error:?} {text:?}")
        });
        let run = crate::native_tests::emit_link_and_run(path, text, "main");
        crate::native_tests::boxed_enum_tests::assert_success(
            &run,
            b"borrow-present\npresent\nborrow-absent\npresent\n",
        );
    }
}
