//! Same expression source, two separate owner roots, then two independent moves.
use super::ordinary_unit;
use lang_frontend::{
    ownership_checking::{
        LoanKind, UnitDropPoint, UnitDropTarget, UnitLoanTarget, UnitValueDeliveryKind,
    },
    parser::Expression,
    type_checking::UnitExpressionId,
};

#[test]
fn distinct_owner_chains_end_before_their_independent_consumes() {
    let checked = ordinary_unit(
        "fun pair(first: String, second: String) {}\nfun run() { val first = \"one\"; val second = \"two\"; pair(view(first), view(second)); consume(first); consume(second) }",
    );
    let expressions = checked.parsed.ast().expressions();
    let node = |id| expressions.get(id).unwrap();
    let text = |span: lang_frontend::source::Span| {
        assert_eq!(span.source_id(), checked.parsed.source_id());
        checked.sources.slice(span).unwrap()
    };
    let call_id = |spelling| {
        let matches = expressions
            .iter()
            .filter_map(|(id, expr)| (text(expr.span()) == spelling).then_some(id))
            .collect::<Vec<_>>();
        let [call] = matches.as_slice() else {
            panic!("one exact call: {spelling}");
        };
        *call
    };
    let arguments = |call| {
        let Expression::Call { arguments, .. } = node(call).payload() else {
            panic!("source fixture is an ordinary call");
        };
        arguments.as_slice()
    };
    let pair_id = call_id("pair(view(first), view(second))");
    let [first, second] = arguments(pair_id) else {
        panic!("two original Borrow operands");
    };
    assert_ne!(first.value, second.value);
    let first_move = call_id("consume(first)");
    let second_move = call_id("consume(second)");
    assert!(node(pair_id).span().end() < node(first_move).span().start());
    assert!(node(first_move).span().end() < node(second_move).span().start());

    for (unit, source_unit) in [checked.memory, checked.rooted]
        .into_iter()
        .zip(checked.source_units)
    {
        assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
        assert!(unit.deferred().is_empty(), "{:?}", unit.deferred());
        let [first_use, second_use] = unit.borrow_results().call_uses() else {
            panic!("one Use for each operand, in original source order");
        };
        let [a, b] = unit.borrow_call_transports() else {
            panic!("one actual retirement for each chain");
        };
        let [first_delivery, second_delivery] = unit.value_deliveries() else {
            panic!("only the two subsequent source moves");
        };
        let qualify = |id| UnitExpressionId::new(source_unit, id);
        let pair = qualify(pair_id);
        let first_root = first_delivery.place().expect("first named move");
        let second_root = second_delivery.place().expect("second named move");
        assert!(first_root.is_root() && second_root.is_root());
        assert_ne!(first_root.root(), second_root.root());

        for (value, call, name, usage, delivery, root) in [
            (
                first.value,
                first_move,
                "first",
                first_use,
                first_delivery,
                first_root,
            ),
            (
                second.value,
                second_move,
                "second",
                second_use,
                second_delivery,
                second_root,
            ),
        ] {
            let [move_arg] = arguments(call) else {
                panic!("one consuming argument");
            };
            assert_eq!(text(node(move_arg.value).span()), name);
            assert_eq!(delivery.call(), qualify(call));
            assert_eq!(delivery.argument(), qualify(move_arg.value));
            assert_eq!(delivery.kind(), UnitValueDeliveryKind::Move);
            assert_eq!(delivery.span(), node(move_arg.value).span());

            let [source_arg] = arguments(value) else {
                panic!("the matching view has one source operand");
            };
            assert_eq!(text(node(value).span()), format!("view({name})"));
            assert_eq!(text(node(source_arg.value).span()), name);
            let inner = qualify(value);
            let source_operand = qualify(source_arg.value);
            let end = [a, b]
                .into_iter()
                .find(|end| end.ended_loans()[0].argument() == inner)
                .expect("operand-qualified transport, not root-only coincidence");
            let [consumer, source] = end.ended_loans();
            assert_eq!((consumer.call(), consumer.argument()), (pair, inner));
            assert_eq!((source.call(), source.argument()), (inner, source_operand));
            assert_eq!((usage.call(), usage.argument()), (pair, inner));
            assert_eq!(usage.expression(), inner);
            assert_eq!(usage.source_loan().call(), inner);
            assert_eq!(usage.source_loan().argument(), source_operand);
            assert_eq!(usage.origin(), &UnitLoanTarget::Place(root.clone()));
            for loan in [consumer, source] {
                assert_eq!(loan.kind(), LoanKind::Shared);
                assert_eq!(loan.target(), usage.origin());
                assert!(unit.loans().contains(loan));
                assert!(
                    !unit
                        .direct_call_loan_ends()
                        .iter()
                        .any(|end| end.loan() == loan)
                );
            }
            assert_eq!(end.handoff_point(), UnitDropPoint::CallReturn(inner));
            assert_eq!(end.point(), UnitDropPoint::CallReturn(pair));
            assert_eq!(usage.end_point(), end.point());
            assert!(
                !unit
                    .drops()
                    .iter()
                    .any(|drop| drop.target() == UnitDropTarget::Temporary(inner)
                        || drop.target() == UnitDropTarget::Named(root.root()))
            );
        }
        assert!(unit.validate().is_ok());
    }
}
