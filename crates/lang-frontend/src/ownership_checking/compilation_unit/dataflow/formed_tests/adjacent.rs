//! Inspect raw dataflow before outer publication gates can hide missing cleanup.
use super::*;

pub(super) const SOURCE: &str = r#"
class Node {}
fun read(value: Node): Int = 1
fun prior(own priorValue: Node?): Int = when (priorValue) { null -> 0; else -> read(priorValue) }
fun view(source: String): borrow String from source = source
fun run(): Unit {
    val source = "kept"
    borrow val item = view(source)
    println(item)
}
"#;

#[test]
fn formed_rejection_clears_adjacent_facts_in_raw_dataflow() {
    let accepted = Fixture::new(&[("adjacent.ko", SOURCE)]);
    let shared = accepted.shared();
    let raw = accepted.dataflow(&accepted.parts(&shared));
    assert!(raw.diagnostics.is_empty(), "{:?}", raw.diagnostics);
    assert!(raw.deferred.is_empty(), "{:?}", raw.deferred);
    assert!(!raw.formed_owned_locals.is_empty());
    assert!(!raw.nullable_whens.is_empty());
    assert!(!raw.borrow_return_origins.is_empty());
    assert!(!raw.borrow_results.bindings.is_empty());
    assert!(!raw.borrow_results.ends.is_empty());

    let text = format!("{SOURCE}\nfun rejected(): Unit {{ val target = 0; target = 1 }}");
    let rejected = Fixture::new(&[("adjacent.ko", &text)]);
    let shared = rejected.shared();
    let raw = rejected.dataflow(&rejected.parts(&shared));
    assert!(raw.deferred.is_empty(), "{:?}", raw.deferred);
    assert_eq!(raw.diagnostics.len(), 1, "{:?}", raw.diagnostics);
    assert_eq!(raw.diagnostics[0].code().to_string(), "L0134");
    let target = rejected.expression(0, "target");
    let expected_span = rejected.parsed[0]
        .ast()
        .expressions()
        .get(target)
        .unwrap()
        .span();
    assert_eq!(raw.diagnostics[0].primary_span(), expected_span);
    assert_eq!(rejected.sources.slice(expected_span).unwrap(), "target");
    assert!(raw.formed_owned_locals.is_empty());
    assert!(raw.nullable_whens.is_empty());
    assert!(raw.borrow_return_origins.is_empty());
    assert!(raw.borrow_results.bindings.is_empty());
    assert!(raw.borrow_results.ends.is_empty());
    assert!(raw.borrow_results.forwarded.is_empty());
    assert!(raw.borrow_results.range_returns.is_empty());
    assert!(raw.borrow_results.range_uses.is_empty());
    assert!(raw.borrow_results.call_uses.is_empty());
}
