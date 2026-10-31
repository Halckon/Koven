//! SPEC-0179 同源 Unit 合同；原源码与事实断言使用独立完整 raw/final 诊断 oracle。
//! 延迟来源的 raw 空诊断与 final 拒绝分别固定，不把恢复事实当作成功准入。
//! 两个入口各自使用真实 source key，不是 multi-source 覆盖。
//! The original name-error/raw-recovery Single witness remains in type_iteration.
//! Final rejection must be Diagnostics with the full oracle; Internal is never success.
use lang_frontend::{
    diagnostic::{Diagnostic, Severity, codes},
    name_resolution::{
        SourceUnitInput, ValidatedCompilationUnitNames, index_compilation_unit,
        resolve_compilation_unit_names,
    },
    parser::ParsedFile,
    source::{SourceId, SourceMap},
    type_checking::{
        CompilationUnitTypes, FinalTypeAdmissionError, check_compilation_unit_types,
        complete_compilation_unit_types, standard_environments,
    },
};
#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

#[derive(Clone, Copy, Debug)]
enum Entry {
    Rooted,
    MemoryDocument,
}
const ENTRIES: [Entry; 2] = [Entry::Rooted, Entry::MemoryDocument];

fn checked(
    entry: Entry,
    text: &str,
    oracle: DiagnosticOracle,
) -> (SourceMap, CompilationUnitTypes) {
    let (sources, _, _, typed) = analyzed_with_oracle(entry, text, oracle);
    (sources, typed)
}

// These positive fixtures have a fixed empty raw/final Diagnostic oracle. Neither
// Unit entry can obtain its expected diagnostics from a Single analysis or its peer.
fn checked_without_diagnostics(entry: Entry, text: &str) -> (SourceMap, CompilationUnitTypes) {
    let (sources, _, _, typed) = analyzed_with_oracle(entry, text, DiagnosticOracle::Empty);
    (sources, typed)
}

#[derive(Clone, Copy)]
enum DiagnosticOracle {
    Empty,
    NonProvider { end: usize },
    BindingMismatch,
    InvalidPattern(usize),
    NamedNonProvider(&'static str),
    BodyError(usize),
    LateBound,
    AmbiguousCall,
    NoMatchingCall,
    ReorderedNonProvider(bool),
    NominalRejection(usize),
    DeferredSource,
}

// Literal fixtures, not diagnostics projected from either analysis entry. The
// SourceId is the only dynamic identity; byte offsets and all text are fixed.
fn rejection_oracle(
    sources: &SourceMap,
    source: SourceId,
    oracle: DiagnosticOracle,
) -> Vec<Diagnostic> {
    let non_provider =
        "for source requires a compiler-bound sequential container, View, IntRange or LongRange";
    let invalid_pattern = "for destructuring requires a concrete value class element";
    let mismatch = "expression type does not match the expected type";
    let (code, message, primary, labels) = match oracle {
        DiagnosticOracle::NonProvider { end } => {
            let text = match end {
                26 => "true",
                28 => "\"text\"",
                27 => "1 + 2",
                _ => panic!("original non-provider source"),
            };
            ("L0159", non_provider, (22, end, text), vec![])
        }
        DiagnosticOracle::BindingMismatch => (
            "L0084",
            mismatch,
            (65, 69, "item"),
            vec![(55, 62, "Boolean", "expected Boolean, found Int")],
        ),
        DiagnosticOracle::InvalidPattern(case) => match case {
            0 => ("L0160", invalid_pattern, (31, 37, "(a, b)"), vec![]),
            1 => (
                "L0160",
                invalid_pattern,
                (45, 51, "(a, b)"),
                vec![(6, 10, "Node", "type declared here")],
            ),
            2 | 3 => (
                "L0118",
                "value class destructuring must bind every component exactly once",
                if case == 2 {
                    (76, 79, "(a)")
                } else {
                    (76, 85, "(a, b, c)")
                },
                vec![(12, 16, "Pair", "expected 2 bindings")],
            ),
            _ => panic!("original pattern source"),
        },
        DiagnosticOracle::NamedNonProvider(name) => {
            let (primary, declaration) = match name {
                "List" => ((69, 71, "xs"), (6, 10, "List")),
                "Iterable" => ((77, 79, "xs"), (6, 14, "Iterable")),
                "Iterator" => ((77, 79, "xs"), (6, 14, "Iterator")),
                _ => panic!("original nominal provider source"),
            };
            (
                "L0159",
                non_provider,
                primary,
                vec![(
                    declaration.0,
                    declaration.1,
                    declaration.2,
                    "type declared here",
                )],
            )
        }
        DiagnosticOracle::BodyError(0) => (
            "L0084",
            mismatch,
            (58, 59, "n"),
            vec![(48, 55, "Boolean", "expected Boolean, found Int")],
        ),
        DiagnosticOracle::BodyError(1) => (
            "L0085",
            "operator does not accept these operand types",
            (27, 28, "+"),
            vec![
                (22, 26, "true", "left operand has type Boolean"),
                (29, 30, "1", "right operand has type Int"),
            ],
        ),
        DiagnosticOracle::BodyError(_) => panic!("original body-error source"),
        DiagnosticOracle::LateBound => (
            "L0115",
            "type argument does not satisfy its Copyable bound",
            (86, 90, "Node"),
            vec![(33, 34, "T", "Copyable bound declared here")],
        ),
        DiagnosticOracle::AmbiguousCall => (
            "L0124",
            "call remains ambiguous after argument type checking",
            (224, 230, "choose"),
            vec![
                (4, 10, "choose", "matching callable declared here"),
                (56, 62, "choose", "matching callable declared here"),
            ],
        ),
        DiagnosticOracle::NoMatchingCall => (
            "L0123",
            "no overload matches the call arguments",
            (224, 230, "choose"),
            vec![],
        ),
        DiagnosticOracle::ReorderedNonProvider(declaration_first) => (
            "L0159",
            non_provider,
            if declaration_first {
                (96, 98, "xs")
            } else {
                (41, 43, "xs")
            },
            vec![],
        ),
        DiagnosticOracle::NominalRejection(case) => {
            let (code, message, primary) = match case {
                0 => ("L0159", non_provider, (44, 46, "xs")),
                1 => ("L0159", non_provider, (45, 47, "xs")),
                2 => ("L0160", invalid_pattern, (46, 49, "(n)")),
                3 => ("L0160", invalid_pattern, (45, 48, "(n)")),
                _ => panic!("original nominal rejection source"),
            };
            (
                code,
                message,
                primary,
                vec![(6, 10, "Node", "type declared here")],
            )
        }
        DiagnosticOracle::DeferredSource => (
            "L0165",
            "type checking could not complete: overload selection is unresolved",
            (58, 68, "::identity"),
            vec![],
        ),
        DiagnosticOracle::Empty => panic!("a fixed rejection oracle is required"),
    };
    let primary_span = sources.span(source, primary.0, primary.1).unwrap();
    assert_eq!(sources.slice(primary_span).unwrap(), primary.2);
    let mut diagnostic = Diagnostic::new(
        sources,
        Severity::Error,
        codes::catalog().unwrap().resolve(code).unwrap(),
        message,
        primary_span,
    )
    .unwrap();
    for (start, end, text, message) in labels {
        let span = sources.span(source, start, end).unwrap();
        assert_eq!(sources.slice(span).unwrap(), text);
        diagnostic.add_label(sources, span, message).unwrap();
    }
    vec![diagnostic]
}

fn analyzed_with_oracle(
    entry: Entry,
    text: &str,
    oracle: DiagnosticOracle,
) -> (
    SourceMap,
    ParsedFile,
    ValidatedCompilationUnitNames,
    CompilationUnitTypes,
) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("iteration.ko", text).unwrap();
    let parsed = parser_test_assertions::parse_file_twice(&sources, source, "iteration");
    assert!(
        parsed.diagnostics().is_empty(),
        "{entry:?}: {:?}",
        parsed.diagnostics()
    );
    let (environment, types) = standard_environments();
    let inputs = [match entry {
        Entry::Rooted => SourceUnitInput::new("root", "iteration.ko", source, &parsed),
        Entry::MemoryDocument => SourceUnitInput::memory_document(source, &parsed),
    }];
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    assert!(
        index.diagnostics().is_empty(),
        "{entry:?}: {:?}",
        index.diagnostics()
    );
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment).unwrap();
    assert!(
        names.diagnostics().is_empty(),
        "{entry:?}: {text}: {:?}",
        names.diagnostics()
    );
    let names = names.validate().unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
    let unit_complete = complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed)
        .map(|complete| complete.is_same_analysis(&typed));
    // Raw recovery and final admission are separate contracts. In particular,
    // the callable-reference sources have no raw diagnostic but must fail final admission.
    let expected_raw = if matches!(
        oracle,
        DiagnosticOracle::Empty | DiagnosticOracle::DeferredSource
    ) {
        vec![]
    } else {
        rejection_oracle(&sources, source, oracle)
    };
    assert_eq!(typed.diagnostics(), expected_raw, "{entry:?}: {text}");
    if matches!(oracle, DiagnosticOracle::Empty) {
        assert!(
            matches!(unit_complete, Ok(true)),
            "{entry:?}: {text}: {unit_complete:?}"
        );
    } else {
        let expected_final = rejection_oracle(&sources, source, oracle);
        match unit_complete {
            Err(FinalTypeAdmissionError::Diagnostics(actual)) => {
                assert_eq!(actual, expected_final, "{entry:?}: {text}");
            }
            other => panic!("expected fixed diagnostics, never success or Internal: {other:?}"),
        }
    }
    let [indexed] = names.names().index().source_units() else {
        panic!("one actual source");
    };
    let [source_names] = names.names().source_units() else {
        panic!("one names product");
    };
    let unit = indexed.id();
    assert_eq!(indexed.source_id(), source);
    assert_eq!(source_names.source_unit(), unit);
    // Source-qualified identities are checked before local IDs are read by individual oracles.
    for plan in typed.sequential_iterations() {
        assert_eq!(plan.statement().source_unit(), unit);
        assert_eq!(plan.source().source_unit(), unit);
        assert_eq!(typed.sequential_iteration(plan.statement()), Some(plan));
        for symbol in plan.binding().symbols() {
            assert_eq!(symbol.source_unit(), unit);
        }
        if let lang_frontend::type_checking::UnitSequentialIterationBinding::Destructure(
            components,
        ) = plan.binding()
        {
            for component in components {
                assert_eq!(component.field().source_unit(), unit);
            }
        }
    }
    (sources, parsed, names, typed)
}

#[test]
fn non_provider_source_is_rejected_at_the_complete_source_span() {
    for entry in ENTRIES {
        for (source, end) in [("true", 26), ("\"text\"", 28), ("1 + 2", 27)] {
            let (sources, _, _, typed) = analyzed_with_oracle(
                entry,
                &format!("fun run() {{ for (_ in {source}) {{}} }}"),
                DiagnosticOracle::NonProvider { end },
            );
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
}

#[test]
fn binding_type_is_available_before_the_body_is_checked() {
    for entry in ENTRIES {
        let (_, _, _, typed) = analyzed_with_oracle(
            entry,
            "fun run(xs: List<Int>) { for (item in xs) { val wrong: Boolean = item } }",
            DiagnosticOracle::BindingMismatch,
        );
        assert_eq!(typed.diagnostics().len(), 1, "{:?}", typed.diagnostics());
    }
}

#[test]
fn range_view_publishes_a_distinct_shared_provider_without_owned_container_identity() {
    for entry in ENTRIES {
        use lang_frontend::type_checking::{IterationProvider, ParameterMode};
        for element in ["String", "Item", "Token"] {
            let (_, typed) = checked_without_diagnostics(
                entry,
                &format!(
                    "class Item(val n:Int){{}}\nclass Token{{deinit(){{}}}}\nfun run(source:View<{element}>):Unit{{for(item in source){{}}}}"
                ),
            );
            assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
            let [plan] = typed.sequential_iterations() else {
                panic!("one provider");
            };
            assert_eq!(plan.provider(), IterationProvider::RangeView);
            assert_eq!(plan.delivery(), ParameterMode::Borrow);
        }
    }
}

#[test]
fn three_providers_publish_name_discard_and_ordered_borrowed_components() {
    for entry in ENTRIES {
        use lang_frontend::type_checking::{
            BuiltinType, ParameterMode, SequentialContainerKind, UnitSequentialIterationBinding,
            UnitTypeKind,
        };
        for (container, provider) in [
            ("Array", SequentialContainerKind::Array),
            ("List", SequentialContainerKind::List),
            ("MutableList", SequentialContainerKind::MutableList),
        ] {
            let text = format!(
                "value class Pair<T>(val first: T, val second: Boolean)\nfun run(xs: {container}<Pair<Int>>) {{ for (item in xs) {{}} for (_ in xs) {{}} for ((n, _) in xs) {{ val i: Int = n }} }}"
            );
            let (_, typed) = checked_without_diagnostics(entry, &text);
            assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
            let plans = typed.sequential_iterations();
            assert_eq!(plans.len(), 3);
            for plan in plans {
                assert_eq!(
                    plan.provider(),
                    lang_frontend::type_checking::IterationProvider::Sequential(provider)
                );
                assert_eq!(plan.delivery(), ParameterMode::Borrow);
                let UnitTypeKind::Intrinsic { arguments, .. } =
                    typed.types().get(plan.source_type()).unwrap()
                else {
                    panic!("intrinsic source")
                };
                assert_eq!(arguments, &[plan.element_type()]);
                if let UnitSequentialIterationBinding::Name(symbol) = plan.binding() {
                    assert_eq!(typed.symbol_type(*symbol), Some(plan.element_type()));
                }
                assert_eq!(typed.sequential_iteration(plan.statement()), Some(plan));
            }
            assert!(matches!(
                plans[0].binding(),
                UnitSequentialIterationBinding::Name(_)
            ));
            assert!(matches!(
                plans[1].binding(),
                UnitSequentialIterationBinding::Discard
            ));
            let UnitSequentialIterationBinding::Destructure(components) = plans[2].binding() else {
                panic!("projection")
            };
            assert_eq!(components.len(), 2);
            assert!(components[0].symbol().is_some());
            assert!(components[1].symbol().is_none());
            assert_eq!(
                typed.types().get(components[0].ty()),
                Some(&UnitTypeKind::Builtin(BuiltinType::Int))
            );
            assert_eq!(
                typed.types().get(components[1].ty()),
                Some(&UnitTypeKind::Builtin(BuiltinType::Boolean))
            );
            let (_, repeated) = checked_without_diagnostics(entry, &text);
            assert_eq!(plans, repeated.sequential_iterations());
            assert!(
                typed.destructurings().is_empty(),
                "borrowed projections cannot become owned destructuring"
            );
        }
    }
}

#[test]
fn invalid_patterns_do_not_publish_partial_plans() {
    for entry in ENTRIES {
        for (case, (prefix, element, pattern, code)) in [
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
        ]
        .into_iter()
        .enumerate()
        {
            let (sources, typed) = checked(
                entry,
                &format!("{prefix}\nfun run(xs: List<{element}>) {{ for ({pattern} in xs) {{}} }}"),
                DiagnosticOracle::InvalidPattern(case),
            );
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
}

#[test]
fn move_only_element_is_typed_without_phase_three_rejections() {
    for entry in ENTRIES {
        let (_, typed) = checked_without_diagnostics(
            entry,
            "class Node {}\nvalue class Pair(val node: Node, val flag: Boolean)\nfun consume(own n: Node) {}\nfun run(xs: List<Pair>) { for ((n, _) in xs) { consume(n) } }",
        );
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        assert_eq!(typed.sequential_iterations().len(), 1);
    }
}

#[test]
fn user_named_types_and_methods_do_not_acquire_provider_identity() {
    for entry in ENTRIES {
        for name in ["List", "Iterable", "Iterator"] {
            let (_, typed) = checked(
                entry,
                &format!(
                    "class {name} {{ fun iterator(): Int = 1 }}\nfun run(xs: {name}) {{ for (_ in xs) {{}} }}"
                ),
                DiagnosticOracle::NamedNonProvider(name),
            );
            assert_eq!(typed.diagnostics().len(), 1, "{:?}", typed.diagnostics());
            assert_eq!(typed.diagnostics()[0].code().to_string(), "L0159");
            assert!(typed.sequential_iterations().is_empty());
        }
    }
}

#[test]
fn body_errors_and_poisoned_source_do_not_publish_plans() {
    for entry in ENTRIES {
        for (case, text) in [
            "fun run(xs: List<Int>) { for (n in xs) { val b: Boolean = n } }",
            "fun run() { for (_ in true + 1) {} }",
        ]
        .into_iter()
        .enumerate()
        {
            let (_, typed) = checked(entry, text, DiagnosticOracle::BodyError(case));
            assert_eq!(typed.diagnostics().len(), 1, "{:?}", typed.diagnostics());
            assert_ne!(typed.diagnostics()[0].code().to_string(), "L0159");
            assert!(typed.sequential_iterations().is_empty());
        }
    }
}

#[test]
fn late_generic_bound_failure_invalidates_iteration_plans() {
    for entry in ENTRIES {
        let (_, typed) = checked(
            entry,
            "class Node {}\nvalue class Holder<T: Copyable>(val payload: T)\nfun run(xs: List<Holder<Node>>) { for ((n) in xs) {} }",
            DiagnosticOracle::LateBound,
        );
        assert!(!typed.diagnostics().is_empty());
        assert!(typed.sequential_iterations().is_empty());
    }
}

#[test]
fn overload_trials_commit_one_iteration_and_one_source_call() {
    for entry in ENTRIES {
        // Both candidate bodies build a for plan before the final expression chooses Int.
        let text = "fun choose(callback: (Int) -> Int): Int = 1
        fun choose(callback: (String) -> String): String = \"text\"
        fun needInt(n: Int): Int = n
        fun source(): List<Int> = listOf(1)
        fun run() { val chosen = choose { for (n in source()) { val x: Int = n } needInt(it) } }";
        let (sources, parsed, _, typed) =
            analyzed_with_oracle(entry, text, DiagnosticOracle::Empty);
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
                        .get(plan.source().expression())
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
            let (_, failed) = checked(
                entry,
                &text.replace("needInt(it)", result),
                if result == "it" {
                    DiagnosticOracle::AmbiguousCall
                } else {
                    DiagnosticOracle::NoMatchingCall
                },
            );
            assert_eq!(failed.diagnostics().len(), 1);
            assert_eq!(
                failed.diagnostics()[0].code().to_string(),
                if result == "it" { "L0124" } else { "L0123" }
            );
            assert!(failed.sequential_iterations().is_empty());
        }
    }
}

#[test]
fn declaration_and_fixture_order_preserve_projected_types_and_diagnostics() {
    for entry in ENTRIES {
        use lang_frontend::type_checking::{
            BuiltinType, UnitSequentialIterationBinding, UnitTypeKind,
        };
        let declaration = "value class Pair<T>(val first: T, val second: Boolean)";
        let body = "fun run(xs: List<Pair<Int>>) { for ((n, flag) in xs) { val i: Int = n\n val b: Boolean = flag } }";
        let fixtures = [
            (format!("{declaration}\n{body}"), true),
            (format!("{body}\n{declaration}"), false),
        ];
        for (text, declaration_first) in fixtures.iter().chain(fixtures.iter().rev()) {
            let (_, _, names, typed) = analyzed_with_oracle(entry, text, DiagnosticOracle::Empty);
            assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
            let plan = &typed.sequential_iterations()[0];
            let UnitSequentialIterationBinding::Destructure(components) = plan.binding() else {
                panic!("components")
            };
            for (component, field, binding, builtin) in [
                (&components[0], "first", "n", BuiltinType::Int),
                (&components[1], "second", "flag", BuiltinType::Boolean),
            ] {
                assert_eq!(
                    names.names().source_units()[0].resolution().symbols()
                        [component.field().symbol().index()]
                    .name(),
                    field
                );
                assert_eq!(
                    names.names().source_units()[0].resolution().symbols()
                        [component.symbol().unwrap().symbol().index()]
                    .name(),
                    binding
                );
                assert_eq!(
                    typed.types().get(component.ty()),
                    Some(&UnitTypeKind::Builtin(builtin))
                );
            }
            let invalid = text.replace("List<Pair<Int>>", "Boolean");
            let (sources, typed) = checked(
                entry,
                &invalid,
                DiagnosticOracle::ReorderedNonProvider(*declaration_first),
            );
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
}

#[test]
fn nominal_rejections_label_the_source_or_element_declaration() {
    for entry in ENTRIES {
        use lang_frontend::diagnostic::DiagnosticDetail;
        for (case, (ty, pattern, code)) in [
            ("Node", "n", "L0159"),
            ("Node?", "n", "L0159"),
            ("List<Node?>", "(n)", "L0160"),
            ("List<Node>", "(n)", "L0160"),
        ]
        .into_iter()
        .enumerate()
        {
            let (sources, typed) = checked(
                entry,
                &format!("class Node {{}}\nfun run(xs: {ty}) {{ for ({pattern} in xs) {{}} }}"),
                DiagnosticOracle::NominalRejection(case),
            );
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
}

#[test]
fn source_modes_and_nested_loops_preserve_unique_source_identities() {
    for entry in ENTRIES {
        for parameter in ["own xs: List<Int>", "xs: List<Int>", "inout xs: List<Int>"] {
            let (_, typed) = checked_without_diagnostics(
                entry,
                &format!(
                    "fun run({parameter}) {{ for (n in xs) {{ for (m in xs) {{ val x: Int = m }} }} }}"
                ),
            );
            assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
            assert_eq!(typed.sequential_iterations().len(), 2);
            assert!(typed.sequential_iterations().windows(2).all(|p| {
                p[0].statement().statement().index() < p[1].statement().statement().index()
            }));
            assert_ne!(
                typed.sequential_iterations()[0].source(),
                typed.sequential_iterations()[1].source()
            );
        }
        let (_, typed) = checked_without_diagnostics(
            entry,
            "class Holder(val xs: List<Int>) {}\nfun run(h: Holder) { for (n in h.xs) { val x: Int = n } }",
        );
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        assert_eq!(typed.sequential_iterations().len(), 1);
    }
}

#[test]
fn deferred_source_suppresses_provider_and_pattern_cascades() {
    for entry in ENTRIES {
        for binding in ["n", "(n, flag)"] {
            let (_, typed) = checked(
                entry,
                &format!(
                    "fun identity(n: Int): Int = n\nfun run() {{ val reference = ::identity\n for ({binding} in listOf(reference(1))) {{}} }}"
                ),
                DiagnosticOracle::DeferredSource,
            );
            assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
            assert!(typed.sequential_iterations().is_empty());
        }
    }
}
