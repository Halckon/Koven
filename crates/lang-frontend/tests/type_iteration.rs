//! SPEC-0179：compiler-bound 借用迭代的 Phase 2 契约。
use lang_frontend::{
    name_resolution::{NameResolution, resolve_names},
    parser::ParsedFile,
    source::SourceMap,
    type_checking::{TypedFile, check_types, standard_environments},
};

#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

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
fn non_provider_source_is_rejected_at_the_complete_source_span() {
    for source in ["true", "\"text\"", "1 + 2"] {
        let (sources, typed) = checked(&format!("fun run() {{ for (_ in {source}) {{}} }}"));
        assert_eq!(typed.diagnostics().len(), 1, "{source}");
        assert_eq!(typed.diagnostics()[0].code().to_string(), "L0159");
        assert_eq!(
            sources
                .slice(typed.diagnostics()[0].primary_span())
                .unwrap(),
            source
        );
    }
}

#[test]
fn binding_type_is_available_before_the_body_is_checked() {
    let (_, typed) =
        checked("fun run(xs: List<Int>) { for (item in xs) { val wrong: Boolean = item } }");
    assert_eq!(typed.diagnostics().len(), 1, "{:?}", typed.diagnostics());
}

#[test]
fn three_providers_publish_name_discard_and_ordered_borrowed_components() {
    use lang_frontend::type_checking::{
        BuiltinType, ParameterMode, SequentialContainerKind, SequentialIterationBinding, TypeKind,
    };
    for (container, provider) in [
        ("Array", SequentialContainerKind::Array),
        ("List", SequentialContainerKind::List),
        ("MutableList", SequentialContainerKind::MutableList),
    ] {
        let text = format!(
            "value class Pair<T>(val first: T, val second: Boolean)\nfun run(xs: {container}<Pair<Int>>) {{ for (item in xs) {{}} for (_ in xs) {{}} for ((n, _) in xs) {{ val i: Int = n }} }}"
        );
        let (_, typed) = checked(&text);
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        let plans = typed.sequential_iterations();
        assert_eq!(plans.len(), 3);
        for plan in plans {
            assert_eq!(plan.provider(), provider);
            assert_eq!(plan.delivery(), ParameterMode::Borrow);
            let TypeKind::Intrinsic { arguments, .. } =
                typed.types().get(plan.source_type()).unwrap()
            else {
                panic!("intrinsic source")
            };
            assert_eq!(arguments, &[plan.element_type()]);
            if let SequentialIterationBinding::Name(symbol) = plan.binding() {
                assert_eq!(typed.symbol_type(*symbol), Some(plan.element_type()));
            }
            assert_eq!(typed.sequential_iteration(plan.statement()), Some(plan));
        }
        assert!(matches!(
            plans[0].binding(),
            SequentialIterationBinding::Name(_)
        ));
        assert!(matches!(
            plans[1].binding(),
            SequentialIterationBinding::Discard
        ));
        let SequentialIterationBinding::Destructure(components) = plans[2].binding() else {
            panic!("projection")
        };
        assert_eq!(components.len(), 2);
        assert!(components[0].symbol().is_some());
        assert!(components[1].symbol().is_none());
        assert_eq!(
            typed.types().get(components[0].ty()),
            Some(&TypeKind::Builtin(BuiltinType::Int))
        );
        assert_eq!(
            typed.types().get(components[1].ty()),
            Some(&TypeKind::Builtin(BuiltinType::Boolean))
        );
        let (_, repeated) = checked(&text);
        assert_eq!(plans, repeated.sequential_iterations());
        assert!(
            typed.destructurings().is_empty(),
            "borrowed projections cannot become owned destructuring"
        );
    }
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
fn move_only_element_is_typed_without_phase_three_rejections() {
    let (_, typed) = checked(
        "class Node {}\nvalue class Pair(val node: Node, val flag: Boolean)\nfun consume(own n: Node) {}\nfun run(xs: List<Pair>) { for ((n, _) in xs) { consume(n) } }",
    );
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.sequential_iterations().len(), 1);
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
fn source_modes_and_nested_loops_preserve_unique_source_identities() {
    for parameter in ["own xs: List<Int>", "xs: List<Int>", "inout xs: List<Int>"] {
        let (_, typed) = checked(&format!(
            "fun run({parameter}) {{ for (n in xs) {{ for (m in xs) {{ val x: Int = m }} }} }}"
        ));
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        assert_eq!(typed.sequential_iterations().len(), 2);
        assert!(
            typed
                .sequential_iterations()
                .windows(2)
                .all(|p| p[0].statement().index() < p[1].statement().index())
        );
        assert_ne!(
            typed.sequential_iterations()[0].source(),
            typed.sequential_iterations()[1].source()
        );
    }
    let (_, typed) = checked(
        "class Holder(val xs: List<Int>) {}\nfun run(h: Holder) { for (n in h.xs) { val x: Int = n } }",
    );
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.sequential_iterations().len(), 1);
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
