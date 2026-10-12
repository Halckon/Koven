//! SPEC-0299 normal source → unmodified LLVM/object → native, without instrumentation.
use super::resource_exchange_tests::run_unit;
use crate::native_owned_map_borrow_cases as cases;
use crate::native_tests::{boxed_enum_tests::assert_success, emit_link_and_run};

fn run(source: &str, expected: &[u8], unit: bool) {
    eprintln!("owned-map-borrow native unit={unit}; source={source}");
    if unit {
        let (provider, entry) = source.split_once("fun entry()").unwrap();
        let provider = format!("package p\n{provider}");
        let consumer = format!("package q\nimport p.*\nfun entry(){entry}");
        for constants in [false, true] {
            let (run, _) = run_unit(&provider, &consumer, constants);
            assert_success(&run, expected);
        }
    } else {
        assert_success(
            &emit_link_and_run("owned-map-borrow.ko", source, "entry"),
            expected,
        );
    }
}
#[test]
fn owned_map_borrow_native_same_source_core() {
    for unit in [false, true] {
        for grouped in [false, true] {
            for (payload, expected) in [
                ("String", "value\nvalue\nvalue\nend\n"),
                (
                    "Text",
                    "borrowed inline\nborrowed inline\nowned inline\nend\n",
                ),
                ("Resource", "value\nvalue\nvalue\nend\ndrop\n"),
            ] {
                run(&cases::core(payload, grouped), expected.as_bytes(), unit);
            }
        }
    }
}
#[test]
fn owned_map_borrow_native_prefix_scopes_missing_and_resource_lifetime() {
    for unit in [false, true] {
        for payload in ["String", "Text", "Resource"] {
            run(&cases::missing(payload, false), b"missing\nend\n", unit);
        }
        run(
            &cases::independent_map(),
            b"value\nvalue\nvalue\nend\n",
            unit,
        );
        for (source, expected) in [
            (cases::PAIR, "prefix\nvalue\nvalue\nvalue\n"),
            (cases::BORROWED_PREFIX, "prefix\nvalue\nvalue\nvalue\n"),
            (
                cases::SCOPES,
                "first\nsecond\nfirst\nsecond\nfirst\nsecond\n",
            ),
            (cases::MISSING, "missing\nend\n"),
            (
                cases::RESOURCE_SCOPE,
                "removed drop\nremoved drop\nend\nremoved drop\nouter drop\n",
            ),
        ] {
            run(source, expected.as_bytes(), unit);
        }
    }
}
#[test]
fn owned_map_borrow_native_last_use_and_separate_inline_read_helpers() {
    for unit in [false, true] {
        for (payload, expected) in [
            ("String", "value\nvalue\nend\n"),
            ("Text", "borrowed inline\nborrowed inline\nend\n"),
            ("Resource", "value\nvalue\nend\ndrop\n"),
        ] {
            run(&cases::last_use(payload), expected.as_bytes(), unit);
        }
        // These helpers exercise different pre-existing callee field-reading paths.
        run(
            &cases::inline_field(unit),
            b"borrowed inline\nborrowed inline\nowned inline\nend\n",
            unit,
        );
        // Explicit source clone reads the String field; it is not an implicit bridge clone.
        run(
            &cases::inline_explicit_clone(),
            b"value\nvalue\nowned inline\nend\n",
            unit,
        );
    }
}
#[test]
fn owned_map_borrow_native_missing_assertion_has_defined_abort() {
    for payload in ["String", "Text", "Resource"] {
        let source = cases::missing(payload, true);
        let run = emit_link_and_run("owned-map-missing.ko", &source, "entry");
        assert!(!run.status.success());
        assert!(run.stdout.is_empty());
        let (provider, entry) = source.split_once("fun entry()").unwrap();
        for constants in [false, true] {
            let (run, _) = run_unit(
                &format!("package p\n{provider}"),
                &format!("package q\nimport p.*\nfun entry(){entry}"),
                constants,
            );
            assert!(!run.status.success());
            assert!(run.stdout.is_empty());
        }
    }
}

#[test]
fn owned_map_borrow_resource_plain_native_baseline_probe() {
    run(
        &cases::core("Resource", false),
        b"value\nvalue\nvalue\nend\ndrop\n",
        false,
    );
}

#[test]
fn owned_map_borrow_non_map_nullable_handle_storage_native_regression() {
    let source = r#"
class Node(val n: Int)
fun observe(node: Node) { if (node.n == 7) { println("node") } else { error("bad node") } }
fun entry() { val node: Node? = Node(7); if (node != null) { observe(node); observe(node) }; val taken = node!!; observe(taken) }
"#;
    // Existing single NullableHandle path, deliberately not the new unit Map bridge.
    run(source, b"node\nnode\nnode\n", false);
}

#[test]
fn owned_map_borrow_single_resource_loop_reuses_static_proof_slot() {
    let source = r#"
class Resource(val text: String) { deinit() { println("drop") } }
fun observe(value: Resource) { println(value.text) }
fun entry() {
 var values = mutableMapOf<Int, Resource>()
 values.put(1, Resource("first".clone())); values.put(2, Resource("second".clone()))
 for (key in arrayOf(1, 2, 3)) {
   val removed = values.remove(key)
   if (removed != null) { observe(removed); observe(removed) } else { println("missing") }
   println("iteration")
 }
 println("end")
}
"#;
    run(source, b"first\nfirst\niteration\ndrop\nsecond\nsecond\niteration\ndrop\nmissing\niteration\nend\n", false);
}
