//! A concrete this receiver keeps its parent capability after its Shared child ends.
use super::*;
use lang_frontend::{
    ownership_checking::{
        UnitReceiverOwnershipFact, UnitReceiverOwnershipKind, UnitReceiverOwnershipTarget,
    },
    type_checking::{UnitCallReceiverOrigin, UnitTypeKind},
};

fn assert_this_receiver(
    observed: &Observed,
    entry: usize,
    receiver: &UnitReceiverOwnershipFact,
    call_text: &str,
) {
    let unit = &observed.units[entry];
    let typed = &observed.unit_types[entry];
    let [source] = observed.unit_names[entry].names().index().source_units() else {
        panic!("one source")
    };
    assert_eq!(receiver.call().source_unit(), source.id());
    let call = observed
        .parsed
        .ast()
        .expressions()
        .get(receiver.call().expression())
        .unwrap();
    assert_eq!(observed.sources.slice(call.span()).unwrap(), call_text);
    assert_eq!(receiver.end_span(), call.span());
    assert!(unit.receiver_facts().contains(receiver));
    assert_eq!(receiver.kind(), UnitReceiverOwnershipKind::SharedLoan);
    assert!(!receiver.is_receiver_reservation());
    let UnitReceiverOwnershipTarget::This(owner) = receiver.target() else {
        panic!("actual this identity")
    };
    assert!(
        matches!(typed.types().types().get(receiver.receiver_type()), Some(UnitTypeKind::Nominal { declaration, .. }) if declaration == owner)
    );
    match receiver.source() {
        UnitCallReceiverOrigin::ImplicitThis(source_owner) => {
            assert_eq!(source_owner, *owner);
            assert!(!call_text.starts_with("this") && !call_text.starts_with('('));
        }
        UnitCallReceiverOrigin::Expression(operand) => {
            assert_eq!(operand.source_unit(), source.id());
            let expression = observed
                .parsed
                .ast()
                .expressions()
                .get(operand.expression())
                .unwrap();
            assert_eq!(receiver.begin_span(), expression.span());
            assert_eq!(
                observed.sources.slice(expression.span()).unwrap(),
                call_text.split('.').next().unwrap()
            );
        }
    }
}

fn assert_this_ends(
    text: &str,
    normal: &[&str],
    prefixes: &[(&str, &str, usize, usize)],
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
        for (event, call) in unit.receiver_call_return_ends().iter().zip(normal) {
            assert_this_receiver(&observed, entry, event.receiver(), call);
            assert_eq!(
                event.point(),
                UnitDropPoint::CallReturn(event.receiver().call())
            );
        }
        for (event, &(call, jump, calls_before, receivers_before)) in
            unit.receiver_call_prefix_ends().iter().zip(prefixes)
        {
            assert_this_receiver(&observed, entry, event.receiver(), call);
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
            assert_eq!(observed.sources.slice(transfer.span()).unwrap(), jump);
            assert_eq!(event.preceding_drops(), 0);
            assert_eq!(event.preceding_call_loan_ends(), calls_before);
            assert_eq!(event.preceding_receiver_loan_ends(), receivers_before);
        }
    }
    observed
}

#[test]
fn original_this_normal_sources_now_observe_their_shared_reborrow_end() {
    // Preserve both exact sources originally excluded by receiver_return.rs.
    for (text, call) in [
        (
            "class Node { fun touch(text: String, own next: Int): Int = next; fun run(): Unit { touch(\"implicit\", 0) } }",
            "touch(\"implicit\", 0)",
        ),
        (
            "class Node { fun touch(text: String, own next: Int): Int = next; fun run(): Unit { this.touch(\"this\", 0) } }",
            "this.touch(\"this\", 0)",
        ),
    ] {
        assert_this_ends(text, &[call], &[]);
    }
}

#[test]
fn original_this_prefix_sources_now_observe_their_shared_reborrow_end() {
    // Preserve both exact sources originally excluded by receiver_prefix.rs.
    for (text, call) in [
        (
            "class Node { fun touch(text: String, own next: Int): Int = next; fun run(): Unit { touch(\"implicit\", return) } }",
            "touch(\"implicit\", return)",
        ),
        (
            "class Node { fun touch(text: String, own next: Int): Int = next; fun run(): Unit { this.touch(\"this\", return) } }",
            "this.touch(\"this\", return)",
        ),
    ] {
        assert_this_ends(text, &[], &[(call, "return", 1, 0)]);
    }
}
