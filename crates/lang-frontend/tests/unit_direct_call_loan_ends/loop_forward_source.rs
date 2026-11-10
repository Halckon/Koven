//! A non-provider Shared parameter can be forwarded across a real iteration return.
use super::*;

#[test]
fn iteration_return_preserves_the_unrelated_parameter_source_token() {
    let observed = observe(
        "fun view(source: String): borrow String from source = source\nfun wrap(source: String): borrow String from source { for (_ in listOf(1)) { return view(source) }; return view(source) }",
    );
    assert_units_admitted(&observed);
    for unit in &observed.units {
        assert_eq!(unit.borrow_results().forwarded_source_loans().len(), 2);
        assert_eq!(unit.loans().len(), 2);
        assert_eq!(
            unit.borrow_forward_transports().len(),
            2,
            "both actually visited return sites carry the verified Shared source"
        );
    }
}

use lang_frontend::ownership_checking::{
    OwnershipBindingKind, UnitIterationCleanupAction as Action,
};

fn span(text: &str, marker: &str) -> ByteSpan {
    let found: Vec<_> = text.match_indices(marker).map(|(start, _)| start).collect();
    let [start] = found.as_slice() else {
        panic!("unique marker {marker:?}: {found:?}")
    };
    (*start, *start + marker.len())
}

fn expected(
    text: &str,
    call: &str,
    argument: &str,
    returned: &str,
    transfer: &str,
    declarations: (&str, &str),
) -> [ByteSpan; 6] {
    let call_bytes = span(text, call);
    let argument_bytes = span(call, argument);
    let prefix = argument.find(" = ").map_or(0, |offset| offset + 3);
    let root = span(text, declarations.0);
    [
        call_bytes,
        (
            call_bytes.0 + argument_bytes.0 + prefix,
            call_bytes.0 + argument_bytes.1,
        ),
        span(text, declarations.1),
        span(text, returned),
        span(text, transfer),
        (
            root.0,
            root.0 + declarations.0.split(':').next().unwrap().len(),
        ),
    ]
}

fn assert_forwarded(observed: &Observed, expected: &[[ByteSpan; 6]]) {
    assert_units_admitted(observed);
    for (unit, names) in observed.units.iter().zip(&observed.unit_names) {
        let [indexed] = names.names().index().source_units() else {
            panic!("one source")
        };
        let [local] = names.names().source_units() else {
            panic!("same-round symbols")
        };
        let bytes = |value: Span| {
            assert_eq!(value.source_id(), observed.parsed.source_id());
            (value.start(), value.end())
        };
        let expression = |id: UnitExpressionId| {
            assert_eq!(id.source_unit(), indexed.id());
            observed
                .parsed
                .ast()
                .expressions()
                .get(id.expression())
                .unwrap()
        };
        let actual: Vec<_> = unit
            .borrow_forward_transports()
            .iter()
            .map(|transport| {
                let loan = transport.source_loan();
                assert_eq!(loan.kind(), LoanKind::Shared);
                assert!(unit.loans().contains(loan));
                let UnitLoanTarget::Place(place) = loan.target() else {
                    panic!("parameter root")
                };
                assert!(place.is_root());
                assert_eq!(place.root().source_unit(), indexed.id());
                assert!(
                    unit.bindings()
                        .iter()
                        .any(|binding| binding.symbol() == place.root()
                            && binding.kind() == OwnershipBindingKind::Shared)
                );
                assert!(!unit.formed_owned_locals().contains(&place.root()));
                assert!(
                    !unit
                        .drops()
                        .iter()
                        .any(|drop| drop.target() == UnitDropTarget::Named(place.root()))
                );
                assert!(
                    unit.borrow_results()
                        .forwarded_source_loans()
                        .iter()
                        .any(|source| source.call() == loan.call()
                            && source.argument() == loan.argument())
                );
                assert!(
                    unit.borrow_return_origins()
                        .iter()
                        .any(
                            |origin| origin.expression() == transport.returned_expression()
                                && origin.origin() == loan.target()
                        )
                );
                assert_eq!(
                    transport.handoff_point(),
                    UnitDropPoint::CallReturn(loan.call())
                );
                assert!(
                    unit.direct_call_loan_ends()
                        .iter()
                        .all(|end| end.loan() != loan)
                );
                assert!(
                    unit.borrow_call_prefix_ends()
                        .iter()
                        .all(|end| end.loan() != loan)
                );
                let UnitDropPoint::ControlTransfer(transfer) = transport.point() else {
                    panic!("real wrapper Return")
                };
                let Expression::Return {
                    value: Some(value), ..
                } = expression(transfer).payload()
                else {
                    panic!("explicit return")
                };
                assert_eq!(*value, transport.returned_expression().expression());
                assert_eq!(loan.begin_span(), expression(loan.argument()).span());
                assert_eq!(loan.end_span(), expression(loan.call()).span());
                [
                    bytes(expression(loan.call()).span()),
                    bytes(expression(loan.argument()).span()),
                    bytes(loan.parameter_span().unwrap()),
                    bytes(expression(transport.returned_expression()).span()),
                    bytes(expression(transfer).span()),
                    bytes(local.resolution().symbols()[place.root().symbol().index()].span()),
                ]
            })
            .collect();
        assert_eq!(
            actual, expected,
            "complete source-qualified forwarding vector"
        );
    }
}

const VIEW: &str = "fun view(source: String): borrow String from source = source\n";

#[test]
fn unrelated_temporary_local_and_parameter_providers_keep_both_return_paths() {
    for (parameters, prefix, provider) in [
        ("", "", "listOf(1)"),
        ("", "val values = listOf(1); ", "values"),
        (", values: List<Int>", "", "values"),
    ] {
        let text = format!(
            "{VIEW}fun wrap(input: String{parameters}): borrow String from input {{ {prefix}for (_ in {provider}) {{ return view(input) }}; return ((view((input)))) }}"
        );
        let observed = observe(&text);
        assert_forwarded(
            &observed,
            &[
                expected(
                    &text,
                    "view(input)",
                    "input",
                    "view(input)",
                    "return view(input)",
                    ("input: String", "source: String"),
                ),
                expected(
                    &text,
                    "view((input))",
                    "(input)",
                    "((view((input))))",
                    "return ((view((input))))",
                    ("input: String", "source: String"),
                ),
            ],
        );
        for unit in &observed.units {
            assert!(
                unit.iteration_cleanup_at(unit.borrow_forward_transports()[0].point())
                    .is_some()
            );
            assert!(
                unit.iteration_cleanup_at(unit.borrow_forward_transports()[1].point())
                    .is_none()
            );
        }
    }
}

#[test]
fn nested_unrelated_providers_keep_exact_inner_to_outer_cleanup_without_source_end() {
    let inner = "for (_ in listOf(2)) { return ((view((input)))) }";
    let outer = format!("for (_ in listOf(1)) {{ {inner} }}");
    let text = format!(
        "{VIEW}fun wrap(input: String): borrow String from input {{ {outer}; return view(input) }}"
    );
    let observed = observe(&text);
    assert_forwarded(
        &observed,
        &[
            expected(
                &text,
                "view((input))",
                "(input)",
                "((view((input))))",
                "return ((view((input))))",
                ("input: String", "source: String"),
            ),
            expected(
                &text,
                "view(input)",
                "input",
                "view(input)",
                "return view(input)",
                ("input: String", "source: String"),
            ),
        ],
    );
    for unit in &observed.units {
        let point = unit.borrow_forward_transports()[0].point();
        let actions = unit
            .iteration_cleanup_at(point)
            .expect("both acquired providers exit");
        let statement_bytes = |id: lang_frontend::type_checking::UnitStatementId| {
            let s = observed
                .parsed
                .ast()
                .statements()
                .get(id.statement())
                .unwrap()
                .span();
            assert_eq!(
                id.source_unit(),
                unit.borrow_forward_transports()[0]
                    .source_loan()
                    .call()
                    .source_unit()
            );
            assert_eq!(s.source_id(), observed.parsed.source_id());
            (s.start(), s.end())
        };
        let actual: Vec<_> = actions
            .iter()
            .map(|action| match action {
                Action::EndElement(statement) => ("element", statement_bytes(*statement)),
                Action::FinishProvider(statement) => ("provider", statement_bytes(*statement)),
                Action::EndSource(statement) => ("source", statement_bytes(*statement)),
                Action::Drop(drop) => {
                    let UnitDropTarget::Temporary(owner) = drop.target() else {
                        panic!("only provider temporary payload drops")
                    };
                    let s = observed
                        .parsed
                        .ast()
                        .expressions()
                        .get(owner.expression())
                        .unwrap()
                        .span();
                    assert_eq!(drop.point(), point);
                    ("drop", (s.start(), s.end()))
                }
                other => panic!("no forwarded source end or invented cleanup: {other:?}"),
            })
            .collect();
        assert_eq!(
            actual,
            [
                ("element", span(&text, inner)),
                ("provider", span(&text, inner)),
                ("source", span(&text, inner)),
                ("drop", span(&text, "listOf(2)")),
                ("element", span(&text, &outer)),
                ("provider", span(&text, &outer)),
                ("source", span(&text, &outer)),
                ("drop", span(&text, "listOf(1)")),
            ]
        );
        let exits: Vec<_> = unit
            .iterations()
            .iter()
            .flat_map(|plan| plan.exits())
            .filter(|exit| exit.point() == point)
            .collect();
        assert_eq!(exits.len(), 2);
        for exit in exits {
            assert_eq!(exit.actions(), actions);
        }
    }
}

#[test]
fn every_active_provider_is_checked_but_finished_providers_do_not_block_forwarding() {
    let view = "fun view(source: List<Int>): borrow List<Int> from source = source\n";
    for body in [
        "for (_ in input) { for (_ in listOf(1)) { return view(input) } }",
        "for (_ in listOf(1)) { for (_ in input) { return view(input) } }",
    ] {
        let text = format!(
            "{view}fun wrap(input: List<Int>): borrow List<Int> from input {{ {body}; return view((input)) }}"
        );
        let observed = observe(&text);
        assert_forwarded(
            &observed,
            &[expected(
                &text,
                "view((input))",
                "(input)",
                "view((input))",
                "return view((input))",
                ("input: List<Int>", "source: List<Int>"),
            )],
        );
        for unit in &observed.units {
            assert_eq!(unit.borrow_results().forwarded_source_loans().len(), 2);
            assert_eq!(
                unit.loans().len(),
                2,
                "excluded active source is still a real loan"
            );
        }
    }
    for jump in ["", "break", "continue"] {
        let text = format!(
            "{view}fun wrap(input: List<Int>): borrow List<Int> from input {{ for (_ in listOf(1)) {{ for (_ in input) {{ {jump} }}; return view(input) }}; return view((input)) }}"
        );
        let observed = observe(&text);
        assert_forwarded(
            &observed,
            &[
                expected(
                    &text,
                    "view(input)",
                    "input",
                    "view(input)",
                    "return view(input)",
                    ("input: List<Int>", "source: List<Int>"),
                ),
                expected(
                    &text,
                    "view((input))",
                    "(input)",
                    "view((input))",
                    "return view((input))",
                    ("input: List<Int>", "source: List<Int>"),
                ),
            ],
        );
        for unit in &observed.units {
            let point = unit.borrow_forward_transports()[0].point();
            let exits = unit
                .iterations()
                .iter()
                .flat_map(|plan| plan.exits())
                .filter(|exit| exit.point() == point)
                .count();
            assert_eq!(
                exits, 1,
                "only the still-active unrelated outer provider exits"
            );
        }
    }
}

#[test]
fn named_same_root_auxiliary_is_not_forwarded_and_owned_auxiliary_still_drops_at_call() {
    for (prefix, auxiliary, expected_drop) in [
        ("", "input", false),
        ("val auxiliary = \"released\"; ", "auxiliary", true),
    ] {
        let call = format!("select(source = input, other = {auxiliary})");
        let text = format!(
            "fun select(other: String, source: String): borrow String from source = source\nfun wrap(input: String): borrow String from input {{ for (_ in listOf(1)) {{ {prefix}return {call} }}; return input }}"
        );
        let observed = observe(&text);
        assert_forwarded(
            &observed,
            &[expected(
                &text,
                &call,
                "source = input",
                &call,
                &format!("return {call}"),
                ("input: String", "source: String"),
            )],
        );
        for unit in &observed.units {
            let transport = &unit.borrow_forward_transports()[0];
            let source = transport.source_loan();
            let actual: Vec<_> = unit
                .loans()
                .iter()
                .filter(|loan| loan.call() == source.call())
                .collect();
            assert_eq!(actual.len(), 2);
            assert_eq!(
                actual
                    .iter()
                    .filter(|loan| loan.argument() == source.argument())
                    .count(),
                1
            );
            let drops: Vec<_> = unit
                .drops()
                .iter()
                .filter(|drop| drop.point() == transport.handoff_point())
                .collect();
            assert_eq!(drops.len(), usize::from(expected_drop));
            if let [drop] = drops.as_slice() {
                let UnitDropTarget::Named(root) = drop.target() else {
                    panic!("owned auxiliary, not source")
                };
                let UnitLoanTarget::Place(source_place) = source.target() else {
                    panic!("source parameter")
                };
                assert_ne!(root, source_place.root());
            }
        }
    }
}

#[test]
fn uncompleted_calls_and_unacquired_provider_sources_keep_distinct_return_contracts() {
    for body in [
        "return select(input, return input)",
        "return input; return select(input, 0)",
    ] {
        let text = format!(
            "fun select(source: String, own n: Int): borrow String from source = source\nfun wrap(input: String): borrow String from input {{ for (_ in listOf(1)) {{ {body} }}; return select(input, 1) }}"
        );
        let observed = observe(&text);
        assert_forwarded(
            &observed,
            &[expected(
                &text,
                "select(input, 1)",
                "input",
                "select(input, 1)",
                "return select(input, 1)",
                ("input: String", "source: String"),
            )],
        );
    }
    let text = format!(
        "{VIEW}fun wrap(input: String, flag: Boolean): borrow String from input {{ for (_ in if (flag) {{ return view(input) }} else {{ listOf(1) }}) {{}}; return view((input)) }}"
    );
    let observed = observe(&text);
    assert_forwarded(
        &observed,
        &[
            expected(
                &text,
                "view(input)",
                "input",
                "view(input)",
                "return view(input)",
                ("input: String", "source: String"),
            ),
            expected(
                &text,
                "view((input))",
                "(input)",
                "view((input))",
                "return view((input))",
                ("input: String", "source: String"),
            ),
        ],
    );
    for unit in &observed.units {
        assert!(
            unit.iteration_cleanup_at(unit.borrow_forward_transports()[0].point())
                .is_none(),
            "source exited before AcquireProvider"
        );
    }
}

#[test]
fn error_and_deferred_sources_revoke_completed_iteration_forwarding() {
    let prefix = format!(
        "{VIEW}fun wrap(input: String): borrow String from input {{ for (_ in listOf(1)) {{ return view(input) }}; return input }}"
    );
    let good = observe(&prefix);
    assert_units_admitted(&good);
    for unit in &good.units {
        assert_eq!(unit.borrow_forward_transports().len(), 1);
    }
    for (suffix, error) in [
        (
            "fun rejected(): Unit { val owner = \"kept\"; val moved = owner; println(owner) }",
            true,
        ),
        (
            "fun rejected(): Unit { var callback: () -> Int = { 1 }; val old = replace(&callback, { 2 }) }",
            false,
        ),
    ] {
        let observed = observe(&format!("{prefix}\n{suffix}"));
        for unit in &observed.units {
            if error {
                assert!(
                    unit.diagnostics()
                        .iter()
                        .any(|diagnostic| diagnostic.code().to_string() == "L0131")
                );
            } else {
                assert!(unit.diagnostics().is_empty());
                assert!(!unit.deferred().is_empty());
            }
            assert!(unit.borrow_forward_transports().is_empty());
            assert!(unit.clone().validate().is_err());
        }
    }
}

#[test]
fn provider_alias_origins_remain_excluded_only_while_their_frames_are_active() {
    for (definition, value_type, declarations) in [
        ("", "List<Int>", "borrow val alias = input;"),
        ("", "List<Int>", "borrow val alias = view(input);"),
        (
            "",
            "List<Int>",
            "borrow val parent = input; borrow val alias = parent;",
        ),
        (
            "class Holder(val items: List<Int>)\n",
            "Holder",
            "borrow val alias = input.items;",
        ),
    ] {
        let text = format!(
            "{definition}fun view(source: {value_type}): borrow {value_type} from source = source\nfun wrap(input: {value_type}): borrow {value_type} from input {{ {declarations} for (_ in alias) {{ return view(input) }}; return view((input)) }}"
        );
        let observed = observe(&text);
        assert_forwarded(
            &observed,
            &[expected(
                &text,
                "view((input))",
                "(input)",
                "view((input))",
                "return view((input))",
                (
                    &format!("input: {value_type}"),
                    &format!("source: {value_type}"),
                ),
            )],
        );
        for unit in &observed.units {
            assert_eq!(unit.borrow_results().forwarded_source_loans().len(), 2);
        }
    }
    for (view, parameters, alias, loop_body, types) in [
        (
            VIEW,
            "input: String, values: List<Int>",
            "borrow val alias = values;",
            "for (_ in alias) { return view(input) }",
            ("input: String", "source: String"),
        ),
        (
            "fun view(source: List<Int>): borrow List<Int> from source = source\n",
            "input: List<Int>",
            "borrow val alias = input;",
            "for (_ in listOf(1)) { for (_ in alias) {}; return view(input) }",
            ("input: List<Int>", "source: List<Int>"),
        ),
    ] {
        let text = format!(
            "{view}fun wrap({parameters}): borrow {} from input {{ {alias} {loop_body}; return view((input)) }}",
            types.0.strip_prefix("input: ").unwrap()
        );
        let observed = observe(&text);
        assert_forwarded(
            &observed,
            &[
                expected(
                    &text,
                    "view(input)",
                    "input",
                    "view(input)",
                    "return view(input)",
                    types,
                ),
                expected(
                    &text,
                    "view((input))",
                    "(input)",
                    "view((input))",
                    "return view((input))",
                    types,
                ),
            ],
        );
    }
}

#[test]
fn cross_source_iteration_returns_preserve_parameter_lineage_under_input_reordering() {
    let mut sources = SourceMap::new();
    let provider_text =
        "package demo\nfun view(source: String): borrow String from source = source";
    let caller_text = "package demo\nfun wrap(input: String, values: List<Int>): borrow String from input { for (_ in values) { return ((view((input)))) }; return view(input) }";
    let provider = sources.add_source("provider.ko", provider_text).unwrap();
    let caller = sources.add_source("caller.ko", caller_text).unwrap();
    let provider_file = parse_file(&sources, &lex(&sources, provider).unwrap()).unwrap();
    let caller_file = parse_file(&sources, &lex(&sources, caller).unwrap()).unwrap();
    assert!(provider_file.diagnostics().is_empty());
    assert!(caller_file.diagnostics().is_empty());
    let (environment, types) = standard_environments();
    for reverse in [false, true] {
        let mut inputs = [
            SourceUnitInput::new("root", "demo/provider.ko", provider, &provider_file),
            SourceUnitInput::new("root", "demo/caller.ko", caller, &caller_file),
        ];
        if reverse {
            inputs.reverse();
        }
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .unwrap()
            .validate()
            .unwrap();
        let raw = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        complete_compilation_unit_types(&sources, &inputs, &names, &types, &raw).unwrap();
        let typed = raw.validate().unwrap();
        let validated = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
            .unwrap()
            .validate()
            .unwrap();
        let unit = validated.ownership();
        let caller_unit = index
            .source_units()
            .iter()
            .find(|unit| unit.source_id() == caller)
            .unwrap()
            .id();
        let provider_unit = index
            .source_units()
            .iter()
            .find(|unit| unit.source_id() == provider)
            .unwrap()
            .id();
        assert_ne!(caller_unit, provider_unit);
        let expression = |id: UnitExpressionId| {
            assert_eq!(id.source_unit(), caller_unit);
            let value = caller_file
                .ast()
                .expressions()
                .get(id.expression())
                .unwrap()
                .span();
            assert_eq!(value.source_id(), caller);
            (value.start(), value.end())
        };
        let transports = unit.borrow_forward_transports();
        assert_eq!(transports.len(), 2);
        for (transport, call, argument, returned, point) in [
            (
                &transports[0],
                "view((input))",
                "(input)",
                "((view((input))))",
                "return ((view((input))))",
            ),
            (
                &transports[1],
                "view(input)",
                "input",
                "view(input)",
                "return view(input)",
            ),
        ] {
            let loan = transport.source_loan();
            assert!(unit.loans().contains(loan));
            assert_eq!(loan.kind(), LoanKind::Shared);
            assert_eq!(expression(loan.call()), span(caller_text, call));
            let call_bytes = span(caller_text, call);
            let argument_bytes = span(call, argument);
            assert_eq!(
                expression(loan.argument()),
                (
                    call_bytes.0 + argument_bytes.0,
                    call_bytes.0 + argument_bytes.1
                )
            );
            let parameter = loan.parameter_span().unwrap();
            assert_eq!(parameter.source_id(), provider);
            assert_eq!(
                (parameter.start(), parameter.end()),
                span(provider_text, "source: String")
            );
            let UnitLoanTarget::Place(place) = loan.target() else {
                panic!("caller Shared parameter")
            };
            assert!(place.is_root());
            assert_eq!(place.root().source_unit(), caller_unit);
            assert!(
                unit.bindings()
                    .iter()
                    .any(|binding| binding.symbol() == place.root()
                        && binding.kind() == OwnershipBindingKind::Shared)
            );
            assert!(
                !unit
                    .drops()
                    .iter()
                    .any(|drop| drop.target() == UnitDropTarget::Named(place.root()))
            );
            assert_eq!(
                expression(transport.returned_expression()),
                span(caller_text, returned)
            );
            let UnitDropPoint::ControlTransfer(transfer) = transport.point() else {
                panic!("actual return")
            };
            assert_eq!(expression(transfer), span(caller_text, point));
        }
        assert_eq!(
            transports[0].source_loan().target(),
            transports[1].source_loan().target()
        );
        assert!(unit.iteration_cleanup_at(transports[0].point()).is_some());
        assert!(unit.iteration_cleanup_at(transports[1].point()).is_none());
    }
}
