//! A later same-root scalar argument is evaluated while the first chain remains live.
use super::ordinary_unit;
use lang_frontend::{
    ownership_checking::{
        LoanKind, UnitDropPoint, UnitDropTarget, UnitIterationCleanupAction, UnitLoanAction,
        UnitLoanTarget, UnitValueDeliveryKind, UnitValueDeliverySource,
    },
    parser::Expression,
    source::Span,
    type_checking::UnitExpressionId,
};

#[test]
fn later_scalar_borrow_ends_before_the_earlier_result_chain() {
    let checked = ordinary_unit(
        "fun pair(first: String, own count: Int) {}\nfun length(source: String): Int = 1\nfun run() { val source = \"kept\"; pair(view(source), length(source)); consume(source) }",
    );
    let expressions = checked.parsed.ast().expressions();
    let node = |id| expressions.get(id).unwrap();
    let text = |span: Span| {
        assert_eq!(span.source_id(), checked.parsed.source_id());
        checked.sources.slice(span).unwrap()
    };
    let unique = |spelling| {
        let matches = expressions
            .iter()
            .filter_map(|(id, expression)| (text(expression.span()) == spelling).then_some(id))
            .collect::<Vec<_>>();
        let [call] = matches.as_slice() else {
            panic!("one source expression {spelling}");
        };
        *call
    };
    let args = |call| {
        let Expression::Call { arguments, .. } = node(call).payload() else {
            panic!("an original call expression");
        };
        arguments.as_slice()
    };
    let pair = unique("pair(view(source), length(source))");
    let consume = unique("consume(source)");
    let [borrow_arg, scalar_arg] = args(pair) else {
        panic!("original Borrow followed by a scalar Value");
    };
    let inner = borrow_arg.value;
    let length = scalar_arg.value;
    let ([source], [scalar_source], [consume_arg]) = (args(inner), args(length), args(consume))
    else {
        panic!("view, length and consume all use one named operand");
    };
    assert_eq!(text(node(inner).span()), "view(source)");
    assert_eq!(text(node(length).span()), "length(source)");
    for name in [source, scalar_source, consume_arg] {
        assert_eq!(text(node(name.value).span()), "source");
    }
    assert_ne!(source.value, scalar_source.value);
    assert!(node(pair).span().end() < node(consume).span().start());

    for (unit, source_unit) in [checked.memory, checked.rooted]
        .into_iter()
        .zip(checked.source_units)
    {
        assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
        assert!(unit.deferred().is_empty(), "{:?}", unit.deferred());
        let qualified = |id| UnitExpressionId::new(source_unit, id);
        let (pair, inner, length, consume) = (
            qualified(pair),
            qualified(inner),
            qualified(length),
            qualified(consume),
        );
        let [usage] = unit.borrow_results().call_uses() else {
            panic!("only the first argument transports a borrowed result");
        };
        let [transport] = unit.borrow_call_transports() else {
            panic!("one actual view→pair transaction");
        };
        let [consumer, source_token] = transport.ended_loans();
        let [direct] = unit.direct_call_loan_ends() else {
            panic!("only length terminates its own independent ordinary loan");
        };
        let length_token = direct.loan();
        assert_eq!((consumer.call(), consumer.argument()), (pair, inner));
        assert_eq!(
            (source_token.call(), source_token.argument()),
            (inner, qualified(source.value))
        );
        assert_eq!(
            (length_token.call(), length_token.argument()),
            (length, qualified(scalar_source.value))
        );
        assert_eq!((usage.call(), usage.argument()), (pair, inner));
        assert_eq!(usage.expression(), inner);
        assert_eq!(usage.source_loan().call(), inner);
        assert_eq!(usage.source_loan().argument(), qualified(source.value));
        assert_eq!(transport.handoff_point(), UnitDropPoint::CallReturn(inner));
        assert_eq!(direct.point(), UnitDropPoint::CallReturn(length));
        assert_eq!(transport.point(), UnitDropPoint::CallReturn(pair));
        assert_eq!(usage.end_point(), transport.point());
        assert_ne!(source_token, length_token);
        for loan in [source_token, length_token, consumer] {
            assert_eq!(loan.kind(), LoanKind::Shared);
            assert_eq!(loan.target(), usage.origin());
            assert!(unit.loans().contains(loan));
        }

        let [scalar, owner] = unit.value_deliveries() else {
            panic!("only the scalar result and subsequent named owner are Values");
        };
        assert_eq!((scalar.call(), scalar.argument()), (pair, length));
        assert_eq!(scalar.source(), &UnitValueDeliverySource::Temporary(length));
        assert_eq!(scalar.kind(), UnitValueDeliveryKind::Temporary);
        assert_eq!(text(scalar.span()), "length(source)");
        assert_eq!(
            (owner.call(), owner.argument()),
            (consume, qualified(consume_arg.value))
        );
        assert_eq!(owner.kind(), UnitValueDeliveryKind::Move);
        let place = owner.place().expect("same named root, moved after pair");
        assert!(place.is_root());
        assert_eq!(usage.origin(), &UnitLoanTarget::Place(place.clone()));

        let [catalog] = unit.loan_action_observations() else {
            panic!("one real action producer for both calls");
        };
        assert_eq!(catalog.source(), source_unit);
        let identify = |predicate: &dyn Fn(&UnitLoanAction) -> bool| {
            let matches = catalog
                .actions()
                .iter()
                .enumerate()
                .filter_map(|(index, action)| predicate(action).then_some(index))
                .collect::<Vec<_>>();
            let [index] = matches.as_slice() else {
                panic!("one exact matching record: {:?}", catalog.actions());
            };
            *index
        };
        let begin = |loan| {
            identify(&|action| matches!(action, UnitLoanAction::Begin(began) if began == loan))
        };
        let source_begin = begin(source_token);
        let consumer_begin = begin(consumer);
        let later_begin = begin(length_token);
        let transfer = identify(&|action| {
            matches!(action,
                UnitLoanAction::BorrowConsumerTransfer { source, consumer, argument }
                    if source == source_token && *consumer == pair && *argument == inner
            )
        });
        let later_end = identify(&|action| {
            matches!(action, UnitLoanAction::Cleanup {
            point, action: UnitIterationCleanupAction::EndCallLoan(ended),
        } if ended == length_token && *point == direct.point())
        });
        let retirement = identify(&|action| {
            matches!(action,
                UnitLoanAction::BorrowConsumerRetired(retirement) if retirement.transaction() == transport
            )
        });
        let UnitLoanAction::BorrowConsumerRetired(retired) = &catalog.actions()[retirement] else {
            unreachable!("matched exact retirement");
        };
        assert_eq!(retired.source_begin(), catalog.action_id(source_begin));
        assert_eq!(retired.consumer_begin(), catalog.action_id(consumer_begin));
        assert_eq!(retired.transfer(), catalog.action_id(transfer));
        assert!(
            source_begin < transfer
                && transfer < consumer_begin
                && consumer_begin < later_begin
                && later_begin < later_end
                && later_end < retirement,
            "straight-line observation: first chain protects the root throughout length"
        );
        assert!(!unit.drops().iter().any(|drop| matches!(drop.target(),
            UnitDropTarget::Temporary(temporary) if temporary == inner || temporary == length)
            || drop.target() == UnitDropTarget::Named(place.root())));
        assert!(unit.validate().is_ok());
    }
}
