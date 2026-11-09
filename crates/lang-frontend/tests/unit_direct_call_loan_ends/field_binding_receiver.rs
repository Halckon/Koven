//! Direct field-initialized borrow bindings preserve the complete canonical source lease.
use super::receiver_return::NODE;
use super::*;
use lang_frontend::{
    ownership_checking::{
        BorrowBindingStorage, UnitReceiverOwnershipFact, UnitReceiverOwnershipKind,
        UnitReceiverOwnershipTarget,
    },
    type_checking::UnitCallReceiverOrigin,
};

struct FieldCall<'a> {
    call: &'a str,
    operand: &'a str,
    path: &'a [&'a str],
    initializer: &'a str,
}
fn field<'a>(
    call: &'a str,
    operand: &'a str,
    path: &'a [&'a str],
    initializer: &'a str,
) -> FieldCall<'a> {
    FieldCall {
        call,
        operand,
        path,
        initializer,
    }
}

fn assert_field(
    observed: &Observed,
    entry: usize,
    receiver: &UnitReceiverOwnershipFact,
    expected: &FieldCall<'_>,
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
    assert_eq!(receiver.call().source_unit(), source.id());
    let node = |id: UnitExpressionId| {
        assert_eq!(id.source_unit(), source.id());
        observed
            .parsed
            .ast()
            .expressions()
            .get(id.expression())
            .unwrap()
    };
    assert_eq!(
        observed
            .sources
            .slice(node(receiver.call()).span())
            .unwrap(),
        expected.call
    );
    assert_eq!(receiver.end_span(), node(receiver.call()).span());
    assert_eq!(receiver.kind(), UnitReceiverOwnershipKind::SharedLoan);
    assert!(!receiver.is_receiver_reservation());
    assert!(unit.receiver_facts().contains(receiver));
    let UnitCallReceiverOrigin::Expression(operand) = receiver.source() else {
        panic!("explicit field receiver")
    };
    assert_eq!(
        observed.sources.slice(node(operand).span()).unwrap(),
        expected.operand
    );
    assert_eq!(receiver.begin_span(), node(operand).span());
    let UnitReceiverOwnershipTarget::Place(place) = receiver.target() else {
        panic!("named field path")
    };
    assert!(!place.is_root());
    assert!(place.elements().is_empty());
    let path: Vec<_> = std::iter::once(place.root())
        .chain(place.fields().iter().copied())
        .map(|symbol| {
            assert_eq!(symbol.source_unit(), source.id());
            names.names().source_units()[symbol.source_unit().index()]
                .resolution()
                .symbols()[symbol.symbol().index()]
            .name()
        })
        .collect();
    assert_eq!(path, expected.path);
    let binding = unit
        .borrow_results()
        .bindings()
        .iter()
        .find(|binding| {
            names.names().source_units()[binding.binding().source_unit().index()]
                .resolution()
                .symbols()[binding.binding().symbol().index()]
            .name()
                == expected.operand.trim_matches(['(', ')'])
        })
        .unwrap();
    assert_eq!(binding.binding().source_unit(), source.id());
    assert_ne!(binding.binding(), place.root());
    assert_eq!(binding.origin(), &UnitLoanTarget::Place(place.clone()));
    assert_eq!(binding.storage(), BorrowBindingStorage::BorrowedStorage);
    assert!(binding.parent().is_none());
    assert!(binding.source_loan().is_none());
    assert_eq!(
        observed
            .sources
            .slice(node(binding.initializer()).span())
            .unwrap(),
        expected.initializer
    );
    assert!(
        unit.drops()
            .iter()
            .all(|drop| drop.target() != UnitDropTarget::Named(binding.binding())),
        "the alias never owns the payload"
    );
}

type Prefix<'a> = (FieldCall<'a>, &'a str, usize, usize);
fn assert_field_ends(text: &str, normal: &[FieldCall<'_>], prefixes: &[Prefix<'_>]) -> Observed {
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
            assert_field(&observed, entry, event.receiver(), expected);
            assert_eq!(
                event.point(),
                UnitDropPoint::CallReturn(event.receiver().call())
            );
        }
        for (event, (expected, jump, calls, receivers)) in
            unit.receiver_call_prefix_ends().iter().zip(prefixes)
        {
            assert_field(&observed, entry, event.receiver(), expected);
            let UnitDropPoint::ControlTransfer(transfer) = event.point() else {
                panic!("actual control transfer")
            };
            assert_eq!(
                transfer.source_unit(),
                event.receiver().call().source_unit()
            );
            let transfer = observed
                .parsed
                .ast()
                .expressions()
                .get(transfer.expression())
                .unwrap();
            assert_eq!(observed.sources.slice(transfer.span()).unwrap(), *jump);
            assert_eq!(event.preceding_drops(), 0);
            assert_eq!(event.preceding_call_loan_ends(), *calls);
            assert_eq!(event.preceding_receiver_loan_ends(), *receivers);
        }
    }
    observed
}

#[test]
fn original_field_binding_normal_source_observes_its_child_end() {
    assert_field_ends(
        &format!(
            "{NODE}class Holder(val node: Node)\nfun run(holder: Holder): Unit {{ borrow val borrowed = holder.node; borrowed.touch(\"field-source\", 0) }}"
        ),
        &[field(
            "borrowed.touch(\"field-source\", 0)",
            "borrowed",
            &["holder", "node"],
            "holder.node",
        )],
        &[],
    );
}

#[test]
fn original_field_binding_return_source_observes_its_child_end() {
    assert_field_ends(
        &format!(
            "{NODE}class Holder(val node: Node)\nfun run(holder: Holder): Unit {{ borrow val borrowed = holder.node; borrowed.touch(\"field-source\", return) }}"
        ),
        &[],
        &[(
            field(
                "borrowed.touch(\"field-source\", return)",
                "borrowed",
                &["holder", "node"],
                "holder.node",
            ),
            "return",
            1,
            0,
        )],
    );
}
