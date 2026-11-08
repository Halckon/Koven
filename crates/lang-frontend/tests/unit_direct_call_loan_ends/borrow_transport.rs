//! Actual source tokens cross inner return before their consumer can retire them.
use super::*;

#[test]
fn nested_source_token_reaches_actual_consumer_before_ending() {
    for operand in ["view(owner)", "((view((owner))))"] {
        let observed = observe(&format!(
            "fun view(source: String): borrow String from source = source\nfun observe(item: String): Unit {{}}\nfun run(): Unit {{ val owner = \"kept\"; observe({operand}) }}"
        ));
        assert_units_admitted(&observed);
        for (unit, names) in observed.units.iter().zip(&observed.unit_names) {
            let [transport] = unit.borrow_call_transports() else {
                panic!("one actual source-to-consumer transport")
            };
            let [indexed] = names.names().index().source_units() else {
                panic!("one source")
            };
            let text = |expression: UnitExpressionId| {
                assert_eq!(expression.source_unit(), indexed.id());
                observed
                    .sources
                    .slice(
                        observed
                            .parsed
                            .ast()
                            .expressions()
                            .get(expression.expression())
                            .unwrap()
                            .span(),
                    )
                    .unwrap()
            };
            let [consumer, source] = transport.ended_loans();
            assert_eq!(
                text(source.call()),
                if operand.starts_with("((") {
                    "view((owner))"
                } else {
                    "view(owner)"
                }
            );
            assert_eq!(text(consumer.call()), format!("observe({operand})"));
            assert_eq!(text(consumer.argument()), operand);
            assert_eq!(consumer.target(), source.target());
            assert_eq!(consumer.kind(), LoanKind::Shared);
            assert_eq!(source.kind(), LoanKind::Shared);
            assert!(unit.loans().contains(consumer));
            assert!(unit.loans().contains(source));
            assert_ne!(source.call(), consumer.call());
            assert_eq!(
                transport.point(),
                UnitDropPoint::CallReturn(consumer.call())
            );
            assert_eq!(
                transport.handoff_point(),
                UnitDropPoint::CallReturn(source.call())
            );
            let [call_use] = unit.borrow_results().call_uses() else {
                panic!("one verified continuation")
            };
            assert_eq!(call_use.source_loan().call(), source.call());
            assert_eq!(call_use.source_loan().argument(), source.argument());
            assert_eq!(call_use.call(), consumer.call());
            assert_eq!(call_use.argument(), consumer.argument());
            let UnitLoanTarget::Place(place) = source.target() else {
                panic!("root")
            };
            let drops: Vec<_> = unit
                .drops()
                .iter()
                .filter(|drop| drop.target() == UnitDropTarget::Named(place.root()))
                .collect();
            assert_eq!(drops.len(), 1);
            assert_eq!(drops[0].point(), transport.point());
        }
    }
}

#[test]
fn uncompleted_consumer_and_later_error_do_not_publish_transport() {
    for tail in [
        "observe(view(owner), return)",
        "observe(view(owner), 0); val moved = owner; println(owner)",
    ] {
        let observed = observe(&format!(
            "fun view(source: String): borrow String from source = source\nfun observe(item: String, own n: Int): Unit {{}}\nfun run(): Unit {{ val owner = \"kept\"; {tail} }}"
        ));
        for unit in &observed.units {
            assert!(unit.borrow_call_transports().is_empty());
        }
    }
}

#[test]
fn named_same_root_consumers_retain_distinct_source_tokens() {
    let observed = observe(
        "fun view(source: String): borrow String from source = source\nfun observe(first: String, second: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; observe(second = view(owner), first = view(owner)) }",
    );
    assert_units_admitted(&observed);
    for unit in &observed.units {
        let transports = unit.borrow_call_transports();
        assert_eq!(transports.len(), 2);
        let bytes = |id: UnitExpressionId| {
            let span = observed
                .parsed
                .ast()
                .expressions()
                .get(id.expression())
                .unwrap()
                .span();
            assert_eq!(span.source_id(), observed.parsed.source_id());
            (span.start(), span.end())
        };
        let actual: Vec<_> = transports
            .iter()
            .map(|transport| {
                let [consumer, source] = transport.ended_loans();
                assert_eq!(source.target(), consumer.target());
                assert_eq!(
                    transport.point(),
                    UnitDropPoint::CallReturn(consumer.call())
                );
                (
                    bytes(consumer.call()),
                    bytes(consumer.argument()),
                    bytes(source.call()),
                    bytes(source.argument()),
                )
            })
            .collect();
        assert_eq!(
            actual,
            [
                ((151, 201), (189, 200), (189, 200), (194, 199)),
                ((151, 201), (168, 179), (168, 179), (173, 178)),
            ]
        );
        assert_ne!(
            transports[0].ended_loans()[1].call(),
            transports[1].ended_loans()[1].call()
        );
    }
}

#[test]
fn deferred_work_revokes_a_nonempty_transport_prefix() {
    let prefix = "fun view(source: String): borrow String from source = source\nfun observe(item: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; observe(view(owner))";
    let good = observe(&format!("{prefix} }}"));
    assert_units_admitted(&good);
    for unit in &good.units {
        assert_eq!(unit.borrow_call_transports().len(), 1);
    }
    let deferred = observe(&format!(
        "{prefix}; var callback: () -> Int = {{ 1 }}; val old = replace(&callback, {{ 2 }}) }}"
    ));
    for unit in &deferred.units {
        assert!(unit.diagnostics().is_empty());
        assert!(!unit.deferred().is_empty());
        assert!(unit.borrow_call_transports().is_empty());
        assert!(unit.clone().validate().is_err());
    }
}

#[test]
fn later_cfg_remains_closed_without_publishing_transport() {
    for tail in ["if (flag) 1 else 2", "if (flag) return else 0"] {
        let observed = observe(&format!(
            "fun view(source: String): borrow String from source = source\nfun observe(item: String, own n: Int): Unit {{}}\nfun run(flag: Boolean): Unit {{ val owner = \"kept\"; observe(view(owner), {tail}) }}"
        ));
        for unit in &observed.units {
            let [diagnostic] = unit.diagnostics() else {
                panic!("existing unproved continuation")
            };
            assert_eq!(diagnostic.code().to_string(), "L0164");
            assert_eq!(
                diagnostic.primary_span().source_id(),
                observed.parsed.source_id()
            );
            assert_eq!(
                (
                    diagnostic.primary_span().start(),
                    diagnostic.primary_span().end()
                ),
                (167, 178)
            );
            assert_eq!(
                observed.sources.slice(diagnostic.primary_span()),
                Ok("view(owner)")
            );
            assert!(unit.borrow_call_transports().is_empty());
            assert!(unit.borrow_results().call_uses().is_empty());
            assert!(unit.clone().validate().is_err());
        }
    }
}

#[test]
fn source_transport_and_temporary_field_end_share_a_completed_consumer() {
    use super::temporary_field_pending::expression;
    use lang_frontend::{
        name_resolution::ReferenceTarget,
        ownership_checking::UnitLoanFact,
        parser::{Item, NameMarker},
        type_checking::Copyability,
    };

    let call_text = "read(view(owner), Holder(\"extra\").text)";
    let observed = observe(&format!(
        "class Holder(val text: String)\nfun view(source: String): borrow String from source = source\nfun read(item: String, extra: String): Unit {{}}\nfun run(): Unit {{ val owner = \"kept\"; {call_text} }}"
    ));
    assert_units_admitted(&observed);
    // 所有预期表达式与参数先从源码 AST 选择，不从 loan、transport 或 drop 反推。
    let call_id = expression(&observed, call_text);
    let source_id = expression(&observed, "view(owner)");
    let owner_id = expression(&observed, "owner");
    let field_id = expression(&observed, "Holder(\"extra\").text");
    let parent_id = expression(&observed, "Holder(\"extra\")");
    let origin_id = expression(&observed, "\"kept\"");
    // Named 初始 owner 的诊断 origin 是声明名；temporary origin 才是完整表达式。
    let owner_origins = observed
        .parsed
        .ast()
        .items()
        .iter()
        .filter_map(|(_, item)| match item.payload() {
            Item::Variable {
                name: NameMarker::Present(span),
                initializer,
                ..
            } if *initializer == origin_id => Some(*span),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [owner_origin] = owner_origins.as_slice() else {
        panic!("one independently selected owner declaration")
    };
    let node = |id| observed.parsed.ast().expressions().get(id).unwrap();
    let Expression::Call { arguments, .. } = node(call_id).payload() else {
        panic!("ordinary outer source call")
    };
    assert_eq!(
        arguments
            .iter()
            .map(|argument| argument.value)
            .collect::<Vec<_>>(),
        [source_id, field_id]
    );
    assert!(
        matches!(node(field_id).payload(), Expression::Member { receiver, .. } if *receiver == parent_id)
    );
    let parameter_span = |text| {
        let spans = observed
            .parsed
            .ast()
            .items()
            .iter()
            .flat_map(|(_, item)| match item.payload() {
                Item::Function { parameters, .. } => parameters.as_slice(),
                _ => &[],
            })
            .filter_map(|parameter| {
                (observed.sources.slice(parameter.span) == Ok(text)).then_some(parameter.span)
            })
            .collect::<Vec<_>>();
        let [span] = spans.as_slice() else {
            panic!("one independently selected parameter for {text}")
        };
        *span
    };
    let source_parameter = parameter_span("source: String");
    let consumer_parameter = parameter_span("item: String");
    let field_parameter = parameter_span("extra: String");

    for (entry, ((unit, names), typed)) in observed
        .units
        .iter()
        .zip(&observed.unit_names)
        .zip(&observed.unit_types)
        .enumerate()
    {
        let [indexed] = names.names().index().source_units() else {
            panic!("one source in each real entry")
        };
        assert_eq!(indexed.source_id(), observed.parsed.source_id());
        match (entry, indexed.key()) {
            (0, SourceUnitKey::Rooted { root, logical_path }) => {
                assert_eq!(root.as_str(), "root");
                assert_eq!(logical_path.as_str(), "call_ends.ko");
            }
            (1, SourceUnitKey::MemoryDocument) => {}
            other => panic!("unexpected source key: {other:?}"),
        }
        let [local] = names.names().source_units() else {
            panic!("same-round local resolution")
        };
        let owner_reference = local
            .resolution()
            .references()
            .iter()
            .find(|reference| reference.span() == node(owner_id).span())
            .unwrap();
        let ReferenceTarget::Symbol(owner_symbol) = owner_reference.target() else {
            panic!("source operand resolves independently to its named owner")
        };
        let qualify = |id| UnitExpressionId::new(indexed.id(), id);
        let returned = UnitDropPoint::CallReturn(qualify(call_id));
        let handed_off = UnitDropPoint::CallReturn(qualify(source_id));
        let parent = qualify(parent_id);
        let parent_type = typed.types().expression_type(parent).unwrap();
        assert_eq!(
            typed.types().copyability(parent_type),
            Copyability::MoveOnly
        );
        let [transport] = unit.borrow_call_transports() else {
            panic!("one real source-to-consumer transport")
        };
        let [consumer, source] = transport.ended_loans();
        assert_eq!(transport.handoff_point(), handed_off);
        assert_eq!(transport.point(), returned);
        let [field_end] = unit.direct_call_loan_ends() else {
            panic!("only the temporary field has a direct end")
        };
        let field = field_end.loan();
        for (loan, call, argument, parameter) in [
            (source, source_id, owner_id, source_parameter),
            (consumer, call_id, source_id, consumer_parameter),
            (field, call_id, field_id, field_parameter),
        ] {
            assert!(unit.loans().contains(loan), "actual registered token");
            assert_eq!(loan.kind(), LoanKind::Shared);
            assert_eq!(loan.call(), qualify(call));
            assert_eq!(loan.argument(), qualify(argument));
            assert_eq!(loan.begin_span(), node(argument).span());
            assert_eq!(loan.end_span(), node(call).span());
            assert_eq!(loan.parameter_span(), Some(parameter));
        }
        let assert_owner = |loan: &UnitLoanFact| {
            let UnitLoanTarget::Place(place) = loan.target() else {
                panic!("the independently resolved named owner")
            };
            assert!(place.is_root());
            assert_eq!(place.root().source_unit(), indexed.id());
            assert_eq!(place.root().symbol(), *owner_symbol);
            assert!(unit.formed_owned_locals().contains(&place.root()));
        };
        assert_owner(source);
        assert_owner(consumer);
        assert_eq!(field.target(), &UnitLoanTarget::Temporary(parent));
        assert_eq!(field_end.point(), returned);
        assert_eq!(field_end.preceding_drops(), 0);
        // 两张 API 没有统一顺序；这里只核各自真实终点和同点 owner 清理。
        let parent_drops = unit
            .drops()
            .iter()
            .filter(|drop| drop.target() == UnitDropTarget::Temporary(parent))
            .map(|drop| (drop.point(), drop.value_origin()))
            .collect::<Vec<_>>();
        assert_eq!(parent_drops, [(returned, node(parent_id).span())]);
        let owner_drops = unit
            .drops()
            .iter()
            .filter(|drop| {
                matches!(drop.target(), UnitDropTarget::Named(root)
                if root.source_unit() == indexed.id() && root.symbol() == *owner_symbol)
            })
            .map(|drop| (drop.point(), drop.value_origin()))
            .collect::<Vec<_>>();
        assert_eq!(owner_drops, [(returned, *owner_origin)]);
        assert_ne!(
            handed_off, returned,
            "inner return cannot clean the source owner"
        );
        assert_eq!(unit.drops().len(), 2, "no separate projected-field drop");
    }
}
