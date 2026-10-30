//! SPEC-0179：compiler-bound 借用迭代的 Phase 2 契约。
#[path = "type_iteration/map_binding_controls/analysis.rs"]
mod map_binding_analysis;

use lang_frontend::{
    name_resolution::{NameResolution, resolve_names},
    parser::ParsedFile,
    source::SourceMap,
    type_checking::{TypedFile, check_types, standard_environments},
};

#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

#[path = "type_iteration/map_borrowed.rs"]
mod map_borrowed;

#[path = "type_iteration/map_binding_controls.rs"]
mod map_binding_controls;

#[path = "type_iteration/pattern_recovery_neighbors.rs"]
mod pattern_recovery_neighbors;

fn checked(text: &str) -> (SourceMap, TypedFile) {
    let (sources, _, _, typed) = analyzed(text);
    (sources, typed)
}

fn analyzed(text: &str) -> (SourceMap, ParsedFile, NameResolution, TypedFile) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("iteration.ko", text).unwrap();
    let parsed = parser_test_assertions::parse_file_twice(&sources, source, "iteration");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let (names, types) = standard_environments();
    let names = resolve_names(&sources, &parsed, &names).unwrap();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let typed = check_types(&sources, &parsed, &names, &types).unwrap();
    (sources, parsed, names, typed)
}

#[test]
fn invalid_patterns_do_not_publish_partial_plans() {
    for (prefix, element, pattern, code) in [
        ("", "Int", "(a, b)", "L0160"),
        ("class Node {}", "Node", "(a, b)", "L0160"),
        (
            "value class Pair(val a: Int, val b: Boolean)",
            "Pair",
            "(a)",
            "L0118",
        ),
        (
            "value class Pair(val a: Int, val b: Boolean)",
            "Pair",
            "(a, b, c)",
            "L0118",
        ),
    ] {
        let (sources, typed) = checked(&format!(
            "{prefix}\nfun run(xs: List<{element}>) {{ for ({pattern} in xs) {{}} }}"
        ));
        assert_eq!(typed.diagnostics().len(), 1, "{:?}", typed.diagnostics());
        assert_eq!(typed.diagnostics()[0].code().to_string(), code);
        assert_eq!(
            sources
                .slice(typed.diagnostics()[0].primary_span())
                .unwrap(),
            pattern
        );
        assert!(typed.sequential_iterations().is_empty());
    }
}

#[test]
fn user_named_types_and_methods_do_not_acquire_provider_identity() {
    for name in ["List", "Iterable", "Iterator"] {
        let (_, typed) = checked(&format!(
            "class {name} {{ fun iterator(): Int = 1 }}\nfun run(xs: {name}) {{ for (_ in xs) {{}} }}"
        ));
        assert_eq!(typed.diagnostics().len(), 1, "{:?}", typed.diagnostics());
        assert_eq!(typed.diagnostics()[0].code().to_string(), "L0159");
        assert!(typed.sequential_iterations().is_empty());
    }
}

#[test]
fn body_errors_and_poisoned_source_do_not_publish_plans() {
    for text in [
        "fun run(xs: List<Int>) { for (n in xs) { val b: Boolean = n } }",
        "fun run() { for (_ in true + 1) {} }",
    ] {
        let (_, typed) = checked(text);
        assert_eq!(typed.diagnostics().len(), 1, "{:?}", typed.diagnostics());
        assert_ne!(typed.diagnostics()[0].code().to_string(), "L0159");
        assert!(typed.sequential_iterations().is_empty());
    }
}

#[test]
fn late_generic_bound_failure_invalidates_iteration_plans() {
    let (_, typed) = checked(
        "class Node {}\nvalue class Holder<T: Copyable>(val payload: T)\nfun run(xs: List<Holder<Node>>) { for ((n) in xs) {} }",
    );
    assert!(!typed.diagnostics().is_empty());
    assert!(typed.sequential_iterations().is_empty());
}

#[test]
fn overload_trials_commit_one_iteration_and_one_source_call() {
    // Both candidate bodies build a for plan before the final expression chooses Int.
    let text = "fun choose(callback: (Int) -> Int): Int = 1
        fun choose(callback: (String) -> String): String = \"text\"
        fun needInt(n: Int): Int = n
        fun source(): List<Int> = listOf(1)
        fun run() { val chosen = choose { for (n in source()) { val x: Int = n } needInt(it) } }";
    let (sources, parsed, _, typed) = analyzed(text);
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(
        typed.sequential_iterations().len(),
        1,
        "failed trial must restore its plan"
    );
    let plan = &typed.sequential_iterations()[0];
    assert_eq!(
        sources
            .slice(
                parsed
                    .ast()
                    .expressions()
                    .get(plan.source())
                    .unwrap()
                    .span()
            )
            .unwrap(),
        "source()"
    );
    assert_eq!(
        typed
            .calls()
            .iter()
            .filter(|call| call.expression() == plan.source())
            .count(),
        1
    );
    for result in ["it", "true"] {
        let (_, failed) = checked(&text.replace("needInt(it)", result));
        assert_eq!(failed.diagnostics().len(), 1);
        assert_eq!(
            failed.diagnostics()[0].code().to_string(),
            if result == "it" { "L0124" } else { "L0123" }
        );
        assert!(failed.sequential_iterations().is_empty());
    }
}

#[test]
fn declaration_and_fixture_order_preserve_projected_types_and_diagnostics() {
    use lang_frontend::type_checking::{BuiltinType, SequentialIterationBinding, TypeKind};
    let declaration = "value class Pair<T>(val first: T, val second: Boolean)";
    let body = "fun run(xs: List<Pair<Int>>) { for ((n, flag) in xs) { val i: Int = n\n val b: Boolean = flag } }";
    let fixtures = [
        format!("{declaration}\n{body}"),
        format!("{body}\n{declaration}"),
    ];
    for text in fixtures.iter().chain(fixtures.iter().rev()) {
        let (_, _, names, typed) = analyzed(text);
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        let plan = &typed.sequential_iterations()[0];
        let SequentialIterationBinding::Destructure(components) = plan.binding() else {
            panic!("components")
        };
        for (component, field, binding, builtin) in [
            (&components[0], "first", "n", BuiltinType::Int),
            (&components[1], "second", "flag", BuiltinType::Boolean),
        ] {
            assert_eq!(names.symbols()[component.field().index()].name(), field);
            assert_eq!(
                names.symbols()[component.symbol().unwrap().index()].name(),
                binding
            );
            assert_eq!(
                typed.types().get(component.ty()),
                Some(&TypeKind::Builtin(builtin))
            );
        }
        let invalid = text.replace("List<Pair<Int>>", "Boolean");
        let (sources, typed) = checked(&invalid);
        assert_eq!(typed.diagnostics().len(), 1);
        assert_eq!(typed.diagnostics()[0].code().to_string(), "L0159");
        assert_eq!(
            sources
                .slice(typed.diagnostics()[0].primary_span())
                .unwrap(),
            "xs"
        );
    }
}

#[test]
fn nominal_rejections_label_the_source_or_element_declaration() {
    use lang_frontend::diagnostic::DiagnosticDetail;
    for (ty, pattern, code) in [
        ("Node", "n", "L0159"),
        ("Node?", "n", "L0159"),
        ("List<Node?>", "(n)", "L0160"),
        ("List<Node>", "(n)", "L0160"),
    ] {
        let (sources, typed) = checked(&format!(
            "class Node {{}}\nfun run(xs: {ty}) {{ for ({pattern} in xs) {{}} }}"
        ));
        assert_eq!(typed.diagnostics()[0].code().to_string(), code);
        let labels = typed.diagnostics()[0]
            .details()
            .iter()
            .filter_map(|detail| match detail {
                DiagnosticDetail::Label(label) => Some(sources.slice(label.span()).unwrap()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(labels, ["Node"]);
    }
}

#[test]
fn deferred_source_suppresses_provider_and_pattern_cascades() {
    for binding in ["n", "(n, flag)"] {
        let (_, typed) = checked(&format!(
            "fun identity(n: Int): Int = n\nfun run() {{ val reference = ::identity\n for ({binding} in listOf(reference(1))) {{}} }}"
        ));
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        assert!(typed.sequential_iterations().is_empty());
    }
}

#[test]
fn upstream_errors_inside_element_types_do_not_publish_recovery_plans() {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source(
            "poison.ko",
            "fun run(xs: List<Missing>) { for (n in xs) {} }",
        )
        .unwrap();
    let parsed = parser_test_assertions::parse_file_twice(&sources, source, "poison");
    let (names, types) = standard_environments();
    let names = resolve_names(&sources, &parsed, &names).unwrap();
    assert_eq!(names.diagnostics().len(), 1);
    let typed = check_types(&sources, &parsed, &names, &types).unwrap();
    assert!(
        typed.diagnostics().is_empty(),
        "no iteration cascade: {:?}",
        typed.diagnostics()
    );
    assert!(typed.sequential_iterations().is_empty());
}
