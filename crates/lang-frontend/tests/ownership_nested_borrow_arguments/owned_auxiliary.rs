//! Value operands do not cut the actual continuing Borrow source chain.
use super::ordinary_unit;
use lang_frontend::{
    ownership_checking::{
        LoanKind, UnitDropPoint, UnitDropTarget, UnitLoanAction, UnitLoanTarget,
        UnitValueDeliveryKind, UnitValueDeliverySource,
    },
    parser::{Expression, FunctionBody, FunctionForm, Item, NameMarker},
    source::Span,
    type_checking::UnitExpressionId,
};

#[test]
fn named_scalar_auxiliary_delivers_value_without_ending_the_source() {
    observe_owned_auxiliary(
        "fun select(own aux: Int, source: String): borrow String from source = source\nfun run() { val source = \"kept\"; observe(select(source = source, aux = 7)); consume(source) }",
        "select(source = source, aux = 7)",
        "7",
        "own aux: Int",
        false,
    );
}

#[test]
fn temporary_resource_auxiliary_is_delivered_and_dropped_only_in_the_callee() {
    observe_owned_auxiliary(
        "class Resource { deinit() {} }\nfun select(source: String, own aux: Resource): borrow String from source = source\nfun run() { val source = \"kept\"; observe(select(source, Resource())); consume(source) }",
        "select(source, Resource())",
        "Resource()",
        "own aux: Resource",
        true,
    );
}

fn observe_owned_auxiliary(
    body: &str,
    inner_spelling: &str,
    auxiliary_spelling: &str,
    parameter_spelling: &str,
    resource: bool,
) {
    let checked = ordinary_unit(body);
    let ast = checked.parsed.ast();
    let node = |id| ast.expressions().get(id).unwrap();
    let text = |span: Span| {
        assert_eq!(span.source_id(), checked.parsed.source_id());
        checked.sources.slice(span).unwrap()
    };
    let unique = |spelling: &str| {
        let matches: Vec<_> = ast
            .expressions()
            .iter()
            .filter_map(|(id, expression)| (text(expression.span()) == spelling).then_some(id))
            .collect();
        let [id] = matches.as_slice() else {
            panic!("expected one actual expression spelled {spelling}");
        };
        *id
    };
    let args = |call| {
        let Expression::Call { arguments, .. } = node(call).payload() else {
            panic!("original call");
        };
        arguments.as_slice()
    };
    let inner = unique(inner_spelling);
    let outer = unique(format!("observe({inner_spelling})").as_str());
    let consume = unique("consume(source)");
    let [source, auxiliary] = args(inner) else {
        panic!("original source-ordered arguments");
    };
    let [operand] = args(outer) else {
        panic!("original sync receiver");
    };
    let [moved] = args(consume) else {
        panic!("original later consume");
    };
    assert_eq!(operand.value, inner);
    assert_eq!(text(node(source.value).span()), "source");
    assert_eq!(text(node(auxiliary.value).span()), auxiliary_spelling);
    assert_eq!(text(node(moved.value).span()), "source");
    assert!(node(outer).span().end() < node(consume).span().start());
    if resource {
        assert!(source.named_prefix.is_none() && auxiliary.named_prefix.is_none());
        assert!(args(auxiliary.value).is_empty());
    } else {
        assert_eq!(text(source.named_prefix.unwrap().name_span), "source");
        assert_eq!(text(auxiliary.named_prefix.unwrap().name_span), "aux");
    }

    let declarations: Vec<_> = ast
        .items()
        .iter()
        .filter_map(|(_, item)| match item.payload() {
            Item::Function {
                name: NameMarker::Present(name),
                parameters,
                form:
                    FunctionForm::Explicit {
                        body: FunctionBody::Expression { expression, .. },
                        ..
                    },
                ..
            } if text(*name) == "select" => Some((parameters, *expression)),
            _ => None,
        })
        .collect();
    let [(parameters, returned)] = declarations.as_slice() else {
        panic!("original select declaration");
    };
    assert_eq!(text(node(*returned).span()), "source");
    let auxiliary_parameters: Vec<_> = parameters
        .iter()
        .filter_map(|param| match param.name {
            NameMarker::Present(name) if text(name) == "aux" => Some(name),
            _ => None,
        })
        .collect();
    let [auxiliary_parameter] = auxiliary_parameters.as_slice() else {
        panic!("original owned auxiliary parameter");
    };

    for ((unit, source_unit), names) in [checked.memory, checked.rooted]
        .into_iter()
        .zip(checked.source_units)
        .zip(checked.unit_names)
    {
        assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
        assert!(unit.deferred().is_empty(), "{:?}", unit.deferred());
        let qualify = |id| UnitExpressionId::new(source_unit, id);
        let (inner, outer, consume) = (qualify(inner), qualify(outer), qualify(consume));
        let [usage] = unit.borrow_results().call_uses() else {
            panic!("only select supplies a borrowed result");
        };
        let [transport] = unit.borrow_call_transports() else {
            panic!("one actual synchronous continuation");
        };
        let [consumer, source_loan] = transport.ended_loans();
        assert_eq!((consumer.call(), consumer.argument()), (outer, inner));
        assert_eq!(
            (source_loan.call(), source_loan.argument()),
            (inner, qualify(source.value))
        );
        assert_eq!((usage.call(), usage.argument()), (outer, inner));
        assert_eq!(usage.expression(), inner);
        assert_eq!(usage.source_loan().call(), inner);
        assert_eq!(usage.source_loan().argument(), qualify(source.value));
        assert_eq!(
            text(source_loan.parameter_span().unwrap()),
            "source: String"
        );
        assert_eq!(transport.handoff_point(), UnitDropPoint::CallReturn(inner));
        assert_eq!(transport.point(), UnitDropPoint::CallReturn(outer));
        assert_eq!(usage.end_point(), transport.point());
        for loan in [consumer, source_loan] {
            assert_eq!(loan.kind(), LoanKind::Shared);
            assert_eq!(loan.target(), usage.origin());
            assert!(unit.loans().contains(loan));
        }
        assert!(unit.direct_call_loan_ends().is_empty());

        let [auxiliary_value, moved_owner] = unit.value_deliveries() else {
            panic!(
                "exactly one auxiliary Value and one later move: {:?}",
                unit.value_deliveries()
            );
        };
        let auxiliary = qualify(auxiliary.value);
        assert_eq!(
            (auxiliary_value.call(), auxiliary_value.argument()),
            (inner, auxiliary)
        );
        assert_eq!(
            auxiliary_value.source(),
            &UnitValueDeliverySource::Temporary(auxiliary)
        );
        assert_eq!(auxiliary_value.kind(), UnitValueDeliveryKind::Temporary);
        assert_eq!(text(auxiliary_value.span()), auxiliary_spelling);
        assert_eq!(
            text(auxiliary_value.parameter_span().unwrap()),
            parameter_spelling
        );
        assert_eq!(
            (moved_owner.call(), moved_owner.argument()),
            (consume, qualify(moved.value))
        );
        assert_eq!(moved_owner.kind(), UnitValueDeliveryKind::Move);
        let root = moved_owner.place().expect("the same named owner is moved");
        assert!(root.is_root());
        assert_eq!(usage.origin(), &UnitLoanTarget::Place(root.clone()));

        let [catalog] = unit.loan_action_observations() else {
            panic!("one source action catalog");
        };
        assert_eq!(catalog.source(), source_unit);
        let retired: Vec<_> = catalog
            .actions()
            .iter()
            .filter_map(|action| match action {
                UnitLoanAction::BorrowConsumerRetired(retired)
                    if retired.transaction() == transport =>
                {
                    Some(retired)
                }
                _ => None,
            })
            .collect();
        let [retired] = retired.as_slice() else {
            panic!("one actual retirement");
        };
        assert_eq!(
            catalog.action(&retired.source_begin().unwrap()),
            Some(&UnitLoanAction::Begin(source_loan.clone()))
        );
        assert_eq!(
            catalog.action(&retired.consumer_begin().unwrap()),
            Some(&UnitLoanAction::Begin(consumer.clone()))
        );
        assert!(matches!(catalog.action(&retired.transfer().unwrap()),
            Some(UnitLoanAction::BorrowConsumerTransfer { source, consumer, argument })
                if source == source_loan && *consumer == outer && *argument == inner));

        // consume's owning String parameter is unrelated; focus on the actual auxiliary,
        // root and result owners rather than counting every drop in the fixture.
        let resolution = names.names().source_units()[source_unit.index()].resolution();
        let auxiliary_symbols: Vec<_> = resolution
            .symbols()
            .iter()
            .filter(|symbol| symbol.span() == *auxiliary_parameter)
            .collect();
        let [auxiliary_symbol] = auxiliary_symbols.as_slice() else {
            panic!("one actual callee parameter identity");
        };
        let auxiliary_drops: Vec<_> = unit
            .drops()
            .iter()
            .filter(|drop| {
                matches!(drop.target(), UnitDropTarget::Named(symbol)
                if symbol.source_unit() == source_unit && symbol.symbol() == auxiliary_symbol.id())
            })
            .collect();
        if resource {
            let [drop] = auxiliary_drops.as_slice() else {
                panic!("one callee-side drop for delivered deinitializing resource");
            };
            assert_eq!(
                drop.point(),
                UnitDropPoint::ControlTransfer(qualify(*returned))
            );
            assert_eq!(drop.value_origin(), *auxiliary_parameter);
            assert!(!drop.is_conditional_temporary());
        } else {
            assert!(auxiliary_drops.is_empty(), "Int Value has no cleanup");
        }
        assert!(!unit.drops().iter().any(|drop| matches!(drop.target(),
            UnitDropTarget::Temporary(temporary) if temporary == auxiliary || temporary == inner)
            || drop.target() == UnitDropTarget::Named(root.root())));
        assert!(unit.validate().is_ok());
    }
}
