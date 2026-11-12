//! Static checker edge evidence for existing leases, not iteration-local borrowing.
#![cfg(feature = "loan-observation-test-support")]
use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    ownership_checking::{
        BorrowLoopEdgeKind, BorrowLoopEdgeObservation, capture_borrow_loop_edges,
        check_compilation_unit_ownership, check_ownership, with_loan_observation,
    },
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, check_types, standard_environments},
};

#[test]
fn ordinary_existing_loan_edges_keep_backedge_and_exit_distinct() {
    for count in [1, 3, 8] {
        let calls = vec!["observe(item)"; count].join("; ");
        for (name, tail, expected) in [
            ("normal", "", Some(BorrowLoopEdgeKind::NormalBackedge)),
            (
                "continue",
                "; continue",
                Some(BorrowLoopEdgeKind::ContinueBackedge),
            ),
            ("break", "; break", Some(BorrowLoopEdgeKind::BreakExit)),
            ("return", "; return", None),
            ("conditional-continue", "; if (flag) { continue }", None),
        ] {
            let body = format!("borrow val item = view(source); while (flag) {{ {calls}{tail} }}");
            check_case(name, &body, expected, false);
        }
    }
}

#[test]
fn ordinary_empty_or_fresh_borrow_cannot_fake_carried_loop_evidence() {
    for (name, body, rejected) in [
        ("empty-entry", "while (flag) { observe(source) }", false),
        (
            "fresh",
            "while (flag) { borrow val item = view(source); observe(item) }",
            false,
        ),
        (
            "nested-scope",
            "borrow val item = view(source); while (flag) { { observe(item) } }",
            false,
        ),
        (
            "local-name",
            "val localFlag = true; borrow val item = view(source); while (localFlag) { observe(item) }",
            false,
        ),
        (
            "constant-false",
            "borrow val item = view(source); while (false) { observe(item) }",
            false,
        ),
        (
            "constant-true",
            "borrow val item = view(source); while (true) { observe(item) }",
            false,
        ),
    ] {
        check_case(name, body, None, rejected);
    }
}

fn check_case(name: &str, body: &str, expected: Option<BorrowLoopEdgeKind>, rejected: bool) {
    let text = format!(
        "fun view(source: String): borrow String from source = source\nfun observe(item: String): Unit {{}}\nfun observe_pair(first: String, second: String): Unit {{}}\nfun run(source: String, other: String, flag: Boolean): Unit {{ {body} }}"
    );
    let mut sources = SourceMap::new();
    let source = sources.add_source("loop-edge.ko", &text).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(parsed.diagnostics().is_empty());
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &parsed, &environment).unwrap();
    assert!(names.diagnostics().is_empty());
    let typed = check_types(&sources, &parsed, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty());
    let (single, edges) =
        capture_borrow_loop_edges(|| check_ownership(&sources, &parsed, &names, &typed).unwrap());
    let baseline = check_ownership(&sources, &parsed, &names, &typed).unwrap();
    assert_eq!(single.diagnostics(), baseline.diagnostics());
    assert_eq!(single.borrow_results(), baseline.borrow_results());
    assert_eq!(single.loans(), baseline.loans());
    assert_eq!(single.loan_ends(), baseline.loan_ends());
    if rejected {
        assert!(
            single
                .diagnostics()
                .iter()
                .any(|d| d.code().to_string() == "L0164")
        );
    } else {
        assert!(
            single.diagnostics().is_empty(),
            "{name}: {:?}",
            single.diagnostics()
        );
    }
    assert_edges(name, &edges, expected);
    assert_actual_consumer_returns(&parsed, &edges, |argument| {
        let lang_frontend::ownership_checking::BorrowLoopExpression::Single { source, expression } =
            argument
        else {
            panic!("single identity")
        };
        assert_eq!(source, parsed.source_id());
        let span = parsed.ast().expressions().get(expression).unwrap().span();
        let reference = names
            .references()
            .iter()
            .find(|reference| reference.span() == span)
            .unwrap();
        let lang_frontend::name_resolution::ReferenceTarget::Symbol(symbol) = reference.target()
        else {
            panic!("resolved binding")
        };
        lang_frontend::ownership_checking::BorrowLoopBinding::Single {
            source,
            symbol: *symbol,
        }
    });
    for rooted in [false, true] {
        let inputs = [if rooted {
            SourceUnitInput::new("root", "loop-edge.ko", source, &parsed)
        } else {
            SourceUnitInput::memory_document(source, &parsed)
        }];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
            .unwrap()
            .validate()
            .unwrap();
        for enabled in [false, true] {
            let (checked, edges) = capture_borrow_loop_edges(|| {
                with_loan_observation(enabled, || {
                    check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                        .unwrap()
                })
            });
            let baseline = with_loan_observation(enabled, || {
                check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap()
            });
            assert_eq!(checked, baseline);
            if rejected {
                assert!(
                    checked
                        .diagnostics()
                        .iter()
                        .any(|d| d.code().to_string() == "L0164")
                );
            } else {
                assert!(
                    checked.diagnostics().is_empty(),
                    "{name}/{rooted}/{enabled}: {:?}",
                    checked.diagnostics()
                );
            }
            assert_edges(name, &edges, expected);
            assert_actual_consumer_returns(&parsed, &edges, |argument| {
                let lang_frontend::ownership_checking::BorrowLoopExpression::Unit(argument) =
                    argument
                else {
                    panic!("unit identity")
                };
                let span = parsed
                    .ast()
                    .expressions()
                    .get(argument.expression())
                    .unwrap()
                    .span();
                let reference = names
                    .names()
                    .references()
                    .iter()
                    .find(|reference| {
                        reference.source_unit() == argument.source_unit()
                            && reference.span() == span
                    })
                    .unwrap();
                let lang_frontend::name_resolution::UnitReferenceTarget::Symbol(symbol) =
                    reference.target()
                else {
                    panic!("resolved unit binding")
                };
                lang_frontend::ownership_checking::BorrowLoopBinding::Unit(*symbol)
            });
        }
    }
}

fn assert_edges(
    name: &str,
    edges: &[BorrowLoopEdgeObservation],
    expected: Option<BorrowLoopEdgeKind>,
) {
    let Some(kind) = expected else {
        assert!(edges.is_empty(), "{name}: {edges:?}");
        return;
    };
    if name == "sibling-loops" {
        assert_eq!(edges.len(), 2);
        assert!(!edges[0].same_loop_as(&edges[1]));
        for event in edges[1].consumer_returns() {
            assert!(!edges[0].owns_consumer_return(event));
        }
        assert_ne!(edges[0].loop_span(), edges[1].loop_span());
        for edge in edges {
            assert_eq!(edge.kind(), kind);
            assert_eq!(edge.carried_binding_count(), 1);
            assert!(edge.preserved() && edge.validate());
        }
        return;
    }
    let [edge] = edges else {
        panic!("{name}: exactly one genuine edge: {edges:?}")
    };
    assert_eq!(edge.kind(), kind);
    assert_eq!(
        edge.carried_binding_count(),
        if matches!(name, "two-roots" | "two-roots-pair") {
            2
        } else {
            1
        }
    );
    assert!(edge.preserved(), "{name}: {edge:?}");
    assert_eq!(
        edge.validate(),
        kind != BorrowLoopEdgeKind::BreakExit,
        "exit is not a repetition proof"
    );
}

#[test]
fn ordinary_two_existing_roots_keep_distinct_loans_on_backedge() {
    check_case(
        "two-roots",
        "borrow val left = view(source); borrow val right = view(other); while (flag) { observe(left); observe(right) }",
        Some(BorrowLoopEdgeKind::NormalBackedge),
        false,
    );
}

#[test]
fn ordinary_sibling_loops_cannot_share_an_analysis_ticket() {
    check_case(
        "sibling-loops",
        "borrow val item = view(source); while (flag) { observe(item) }; while (flag) { observe(item) }",
        Some(BorrowLoopEdgeKind::NormalBackedge),
        false,
    );
}

fn assert_actual_consumer_returns(
    parsed: &lang_frontend::parser::ParsedFile,
    edges: &[BorrowLoopEdgeObservation],
    resolve: impl Fn(
        lang_frontend::ownership_checking::BorrowLoopExpression,
    ) -> lang_frontend::ownership_checking::BorrowLoopBinding,
) {
    use lang_frontend::{
        ownership_checking::BorrowLoopExpression,
        parser::{Expression, Statement},
    };
    for edge in edges {
        let (_, node) = parsed
            .ast()
            .statements()
            .iter()
            .find(|(_, node)| node.span() == edge.loop_span())
            .unwrap();
        let Statement::While { body, .. } = node.payload() else {
            panic!("actual while")
        };
        let (Statement::Block { elements } | Statement::ControlBody { elements }) =
            parsed.ast().statements().get(*body).unwrap().payload()
        else {
            panic!("actual body")
        };
        let expected = elements
            .iter()
            .filter_map(|statement| {
                let Statement::Expression { expression } =
                    parsed.ast().statements().get(*statement).unwrap().payload()
                else {
                    return None;
                };
                match parsed
                    .ast()
                    .expressions()
                    .get(*expression)
                    .unwrap()
                    .payload()
                {
                    Expression::Call { arguments, .. } => Some((*expression, arguments)),
                    _ => None,
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(
            edge.consumer_returns().len(),
            expected.len(),
            "one independent real return for every consumer, not just equal final states"
        );
        for (event, (call, arguments)) in edge.consumer_returns().iter().zip(expected) {
            assert!(edge.owns_consumer_return(event));
            let raw = |id| match id {
                BorrowLoopExpression::Single { source, expression } => {
                    assert_eq!(source, parsed.source_id());
                    expression
                }
                BorrowLoopExpression::Unit(id) => id.expression(),
            };
            assert_eq!(raw(event.call()), call);
            assert_eq!(event.arguments().len(), arguments.len());
            assert_eq!(event.released_loan_count(), arguments.len());
            for ((actual, binding), expected) in event.arguments().iter().zip(arguments) {
                assert_eq!(raw(*actual), expected.value);
                assert_eq!(*binding, resolve(*actual));
            }
        }
    }
}

#[test]
fn ordinary_call_return_is_per_operand_not_per_root() {
    check_case(
        "repeated-root-pair",
        "borrow val item = view(source); while (flag) { observe_pair(item, item) }",
        Some(BorrowLoopEdgeKind::NormalBackedge),
        false,
    );
    check_case(
        "two-roots-pair",
        "borrow val left = view(source); borrow val right = view(other); while (flag) { observe_pair(left, right); continue }",
        Some(BorrowLoopEdgeKind::ContinueBackedge),
        false,
    );
}
