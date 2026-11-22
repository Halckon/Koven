//! Original synchronous argument fixture; bind the real pending-loan removal, not a descriptor.
use super::super::origin_probe::Event;
use super::*;
use crate::{
    name_resolution::{SourceUnitKey, UnitSymbolId},
    ownership_checking::{
        LoanKind, UnitDropPoint, UnitDropTarget, UnitLoanAction, UnitLoanTarget,
        UnitValueDeliveryKind,
    },
    parser::{Expression, Item, NameMarker},
};

#[test]
fn borrowed_auxiliary_retires_before_its_owner_and_outer_consumer() {
    let text = "fun view(source: String): borrow String from source = source\nfun observe(item: String) { println(item) }\nfun consume(own item: String) {}\nfun select(aux: String, source: String): borrow String from source = source\nfun run() { val source = \"kept\"; val aux = \"aux\"; observe(select(source = source, aux = aux)); consume(source) }";

    for rooted in [false, true] {
        let fixture = Fixture::standard_source("nested_borrow.ko", text, rooted);
        let [entry] = fixture.names.names().index().source_units() else {
            panic!("one source");
        };
        assert_eq!(fixture.sources.source_text(entry.source_id()), Ok(text));
        match (rooted, entry.key()) {
            (false, SourceUnitKey::MemoryDocument) => {}
            (true, SourceUnitKey::Rooted { root, logical_path }) => {
                assert_eq!(root.as_str(), "root");
                assert_eq!(logical_path.as_str(), "nested_borrow.ko");
            }
            _ => panic!("original requested Unit layout"),
        }
        let qualify = |id| UnitExpressionId::new(entry.id(), id);
        let ast = fixture.parsed[0].ast();
        let node = |id| ast.expressions().get(id).unwrap();
        let slice = |span| fixture.sources.slice(span).unwrap();
        let args = |call| {
            let Expression::Call { arguments, .. } = node(call).payload() else {
                panic!("original call");
            };
            arguments.as_slice()
        };
        let select = fixture.expression(0, "select(source = source, aux = aux)");
        let observe = fixture.expression(0, "observe(select(source = source, aux = aux))");
        let consume = fixture.expression(0, "consume(source)");
        let [source, auxiliary] = args(select) else {
            panic!("original source order");
        };
        let [consumer_operand] = args(observe) else {
            panic!("one actual consumer");
        };
        let [consume_operand] = args(consume) else {
            panic!("the later owner move");
        };
        assert_eq!(consumer_operand.value, select);
        assert_eq!(slice(source.named_prefix.unwrap().name_span), "source");
        assert_eq!(slice(auxiliary.named_prefix.unwrap().name_span), "aux");
        assert_eq!(slice(node(source.value).span()), "source");
        assert_eq!(slice(node(auxiliary.value).span()), "aux");
        assert_eq!(slice(node(consume_operand.value).span()), "source");
        let local = |spelling, initializer| {
            let markers: Vec<_> = ast
                .items()
                .iter()
                .filter_map(|(_, item)| match item.payload() {
                    Item::Variable {
                        name: NameMarker::Present(marker),
                        initializer: value,
                        ..
                    } if slice(*marker) == spelling
                        && slice(node(*value).span()) == initializer =>
                    {
                        Some(*marker)
                    }
                    _ => None,
                })
                .collect();
            let [marker] = markers.as_slice() else {
                panic!("one original local declaration");
            };
            let symbols = fixture.names.names().source_units()[entry.id().index()]
                .resolution()
                .symbols()
                .iter()
                .filter(|symbol| symbol.span() == *marker)
                .collect::<Vec<_>>();
            let [symbol] = symbols.as_slice() else {
                panic!("one actual local symbol");
            };
            UnitSymbolId::new(entry.id(), symbol.id())
        };
        let source_root = local("source", "\"kept\"");
        let aux_root = local("aux", "\"aux\"");
        assert_ne!(source_root, aux_root);

        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
        assert!(owned.clone().validate().is_ok());
        let select = qualify(select);
        let observe = qualify(observe);
        let [usage] = owned.borrow_results().call_uses() else {
            panic!("one ordinary synchronous use");
        };
        let [transport] = owned.borrow_call_transports() else {
            panic!("one actual carried chain");
        };
        let [consumer, source_loan] = transport.ended_loans();
        assert_eq!(
            (usage.expression(), usage.call(), usage.argument()),
            (select, observe, select)
        );
        assert_eq!(
            (usage.source_loan().call(), usage.source_loan().argument()),
            (select, qualify(source.value))
        );
        assert_eq!(
            (source_loan.call(), source_loan.argument()),
            (select, qualify(source.value))
        );
        assert_eq!((consumer.call(), consumer.argument()), (observe, select));
        assert_eq!(source_loan.target(), usage.origin());
        assert_eq!(consumer.target(), usage.origin());
        assert!(
            matches!(usage.origin(), UnitLoanTarget::Place(place) if place.is_root() && place.root() == source_root)
        );
        assert_eq!(transport.handoff_point(), UnitDropPoint::CallReturn(select));
        assert_eq!(transport.point(), UnitDropPoint::CallReturn(observe));
        assert_eq!(usage.end_point(), transport.point());
        let matching: Vec<_> = owned
            .loans()
            .iter()
            .filter(|loan| loan.call() == select && loan.argument() == qualify(auxiliary.value))
            .collect();
        let [aux_loan] = matching.as_slice() else {
            panic!("distinct actual auxiliary token");
        };
        assert!(
            matches!(aux_loan.target(), UnitLoanTarget::Place(place) if place.is_root() && place.root() == aux_root)
        );
        for loan in [source_loan, *aux_loan, consumer] {
            assert_eq!(loan.kind(), LoanKind::Shared);
            assert!(owned.loans().contains(loan));
        }
        assert_eq!(
            slice(source_loan.parameter_span().unwrap()),
            "source: String"
        );
        assert_eq!(slice(aux_loan.parameter_span().unwrap()), "aux: String");
        assert_eq!(slice(consumer.parameter_span().unwrap()), "item: String");

        let [catalog] = owned.loan_action_observations() else {
            panic!("one actual catalog");
        };
        let retirees: Vec<_> = catalog
            .actions()
            .iter()
            .filter_map(|action| match action {
                UnitLoanAction::BorrowConsumerRetired(retirement)
                    if retirement.transaction() == transport =>
                {
                    Some(retirement)
                }
                _ => None,
            })
            .collect();
        let [retirement] = retirees.as_slice() else {
            panic!("one producer retirement");
        };
        assert_eq!(
            catalog.action(&retirement.source_begin().unwrap()),
            Some(&UnitLoanAction::Begin(source_loan.clone()))
        );
        assert_eq!(
            catalog.action(&retirement.consumer_begin().unwrap()),
            Some(&UnitLoanAction::Begin(consumer.clone()))
        );
        assert!(matches!(catalog.action(&retirement.transfer().unwrap()),
            Some(UnitLoanAction::BorrowConsumerTransfer { source, consumer, argument })
                if source == source_loan && *consumer == observe && *argument == select));

        let [delivery] = owned.value_deliveries() else {
            panic!("only the final consume moves");
        };
        assert_eq!(delivery.call(), qualify(consume));
        assert_eq!(delivery.argument(), qualify(consume_operand.value));
        assert_eq!(delivery.kind(), UnitValueDeliveryKind::Move);
        assert!(
            matches!(delivery.place(), Some(place) if place.is_root() && place.root() == source_root)
        );
        let aux_drops: Vec<_> = owned
            .drops()
            .iter()
            .filter(|drop| drop.target() == UnitDropTarget::Named(aux_root))
            .collect();
        let [aux_drop] = aux_drops.as_slice() else {
            panic!("auxiliary still has one independent cleanup");
        };
        assert_eq!(aux_drop.point(), UnitDropPoint::CallReturn(select));
        assert!(
            !owned
                .drops()
                .iter()
                .any(|drop| drop.target() == UnitDropTarget::Temporary(select)
                    || drop.target() == UnitDropTarget::Named(source_root))
        );

        compare_and_inspect(&fixture, &fixture.parts(&owned), |checker| {
            let make = || {
                super::super::super::DropPlanner::new(
                    checker,
                    super::super::super::liveness::build(checker, true).unwrap(),
                )
            };
            let baseline = make().run().unwrap();
            let mut traced = make();
            traced.origin_probe = Some(Vec::new());
            let traced = traced.run().unwrap();
            assert_eq!(baseline.facts, traced.facts);
            assert_eq!(
                baseline.iteration_actions.consumer_transports(),
                traced.iteration_actions.consumer_transports()
            );
            assert_eq!(
                traced.iteration_actions.consumer_transports(),
                std::slice::from_ref(transport)
            );
            assert_eq!(
                baseline
                    .iteration_actions
                    .finish(entry.id(), entry.source_id()),
                traced
                    .iteration_actions
                    .finish(entry.id(), entry.source_id())
            );

            // The existing plain probe records actual pending removals. It does
            // not expose the transport retirement; that comes from the real catalog above.
            let events = traced
                .origin_probe
                .unwrap()
                .into_iter()
                .filter(|event| match event {
                    Event::BorrowRegistered(loan)
                    | Event::PendingBorrowRemoved { loan, .. }
                    | Event::DirectBorrowEnded { loan, .. }
                    | Event::UnwindBorrowRemoved { loan, .. } => {
                        [source_loan, *aux_loan, consumer].contains(&loan)
                    }
                    Event::Dropped(drop) => drop == *aux_drop,
                    _ => false,
                })
                .collect::<Vec<_>>();
            assert_eq!(
                events,
                [
                    Event::BorrowRegistered(source_loan.clone()),
                    Event::BorrowRegistered((*aux_loan).clone()),
                    Event::PendingBorrowRemoved {
                        loan: (*aux_loan).clone(),
                        call: select
                    },
                    Event::Dropped(**aux_drop),
                    Event::BorrowRegistered(consumer.clone()),
                ]
            );
        });
    }
}
