//! A direct borrow binding's receiver child ends without inventing a payload address.
use super::receiver_return::NODE;
use super::*;
use lang_frontend::{
    ownership_checking::{
        BorrowBindingStorage, UnitReceiverOwnershipFact, UnitReceiverOwnershipKind,
        UnitReceiverOwnershipTarget,
    },
    type_checking::UnitCallReceiverOrigin,
};

struct BindingCall<'a> {
    call: &'a str,
    operand: &'a str,
    source: &'a str,
}
fn end<'a>(call: &'a str, operand: &'a str, source: &'a str) -> BindingCall<'a> {
    BindingCall {
        call,
        operand,
        source,
    }
}

fn assert_binding_receiver(
    observed: &Observed,
    entry: usize,
    receiver: &UnitReceiverOwnershipFact,
    expected: &BindingCall<'_>,
) {
    let unit = &observed.units[entry];
    let names = &observed.unit_names[entry];
    let [source] = names.names().index().source_units() else {
        panic!("one source")
    };
    assert_eq!(source.source_id(), observed.parsed.source_id());
    match (entry, source.key()) {
        (0, SourceUnitKey::Rooted { root, logical_path }) => {
            assert_eq!(root.as_str(), "root");
            assert_eq!(logical_path.as_str(), "call_ends.ko");
        }
        (1, SourceUnitKey::MemoryDocument) => {}
        other => panic!("unexpected source: {other:?}"),
    }
    let node = |id: UnitExpressionId| {
        assert_eq!(id.source_unit(), source.id());
        observed
            .parsed
            .ast()
            .expressions()
            .get(id.expression())
            .unwrap()
    };
    let symbol_name = |symbol: lang_frontend::name_resolution::UnitSymbolId| {
        assert_eq!(symbol.source_unit(), source.id());
        names.names().source_units()[symbol.source_unit().index()]
            .resolution()
            .symbols()[symbol.symbol().index()]
        .name()
    };
    assert!(unit.receiver_facts().contains(receiver));
    assert_eq!(receiver.kind(), UnitReceiverOwnershipKind::SharedLoan);
    assert!(!receiver.is_receiver_reservation());
    assert_eq!(
        observed
            .sources
            .slice(node(receiver.call()).span())
            .unwrap(),
        expected.call
    );
    assert_eq!(receiver.end_span(), node(receiver.call()).span());
    let UnitCallReceiverOrigin::Expression(operand) = receiver.source() else {
        panic!("explicit binding receiver")
    };
    assert_eq!(
        observed.sources.slice(node(operand).span()).unwrap(),
        expected.operand
    );
    assert_eq!(receiver.begin_span(), node(operand).span());
    let UnitReceiverOwnershipTarget::Place(place) = receiver.target() else {
        panic!("canonical source lease")
    };
    assert!(place.is_root());
    assert_eq!(symbol_name(place.root()), expected.source);
    let binding_name = expected.operand.trim_matches(['(', ')']);
    let binding = unit
        .borrow_results()
        .bindings()
        .iter()
        .find(|b| symbol_name(b.binding()) == binding_name)
        .unwrap();
    assert_ne!(
        binding.binding(),
        place.root(),
        "operand binding is not the source lease root"
    );
    assert_eq!(binding.origin(), &UnitLoanTarget::Place(place.clone()));
    assert_eq!(binding.storage(), BorrowBindingStorage::BorrowedStorage);
    assert!(binding.parent().is_none());
    assert!(binding.source_loan().is_none());
    assert_eq!(
        observed
            .sources
            .slice(node(binding.initializer()).span())
            .unwrap()
            .trim_matches(['(', ')']),
        expected.source
    );
    assert!(
        unit.drops()
            .iter()
            .all(|drop| drop.target() != UnitDropTarget::Named(binding.binding())),
        "the borrow binding never owns the payload"
    );
}

type Prefix<'a> = (BindingCall<'a>, &'a str, usize, usize);
fn assert_binding_ends(
    text: &str,
    normal: &[BindingCall<'_>],
    prefixes: &[Prefix<'_>],
) -> Observed {
    let observed = observe(text);
    assert_units_admitted(&observed);
    for (entry, unit) in observed.units.iter().enumerate() {
        assert_eq!(
            unit.receiver_call_return_ends().len(),
            normal.len(),
            "complete normal vector: {text}"
        );
        assert_eq!(
            unit.receiver_call_prefix_ends().len(),
            prefixes.len(),
            "complete prefix vector: {text}"
        );
        for (event, expected) in unit.receiver_call_return_ends().iter().zip(normal) {
            assert_binding_receiver(&observed, entry, event.receiver(), expected);
            assert_eq!(
                event.point(),
                UnitDropPoint::CallReturn(event.receiver().call())
            );
        }
        for (event, (expected, jump, calls, receivers)) in
            unit.receiver_call_prefix_ends().iter().zip(prefixes)
        {
            assert_binding_receiver(&observed, entry, event.receiver(), expected);
            let UnitDropPoint::ControlTransfer(transfer) = event.point() else {
                panic!("actual jump")
            };
            assert_eq!(
                transfer.source_unit(),
                event.receiver().call().source_unit()
            );
            let node = observed
                .parsed
                .ast()
                .expressions()
                .get(transfer.expression())
                .unwrap();
            assert_eq!(observed.sources.slice(node.span()).unwrap(), *jump);
            assert_eq!(event.preceding_drops(), 0);
            assert_eq!(event.preceding_call_loan_ends(), *calls);
            assert_eq!(event.preceding_receiver_loan_ends(), *receivers);
        }
    }
    observed
}

#[test]
fn direct_binding_normal_completion_ends_the_actual_child_receiver() {
    assert_binding_ends(
        &format!(
            "{NODE}fun run(own node: Node): Unit {{ borrow val borrowed = node; borrowed.touch(\"direct\", 0) }}"
        ),
        &[end("borrowed.touch(\"direct\", 0)", "borrowed", "node")],
        &[],
    );
}

#[test]
fn direct_binding_return_prefix_ends_the_actual_child_receiver() {
    assert_binding_ends(
        &format!(
            "{NODE}fun run(own node: Node): Unit {{ borrow val borrowed = node; borrowed.touch(\"direct\", return) }}"
        ),
        &[],
        &[(
            end("borrowed.touch(\"direct\", return)", "borrowed", "node"),
            "return",
            1,
            0,
        )],
    );
}
