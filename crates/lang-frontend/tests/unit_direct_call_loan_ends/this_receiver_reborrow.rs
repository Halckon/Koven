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

#[test]
fn grouped_this_and_callable_modes_preserve_the_real_child_token() {
    for declaration in ["class Node", "value class Node(val value: Int)"] {
        for mode in ["", "inout ", "own "] {
            for (open, close, next) in [
                ("", "", "0"),
                ("", "", "return"),
                ("loop {", "}", "break"),
                ("while (flag) {", "}", "continue"),
            ] {
                let call = format!("((this)).touch(\"group\", {next})");
                let text = format!(
                    "{declaration} {{ fun touch(text: String, own next: Int): Int = next; {mode}fun run(flag: Boolean): Unit {{ {open} {call} {close} }} }}"
                );
                let observed = if next == "0" {
                    assert_this_ends(&text, &[&call], &[])
                } else {
                    assert_this_ends(&text, &[], &[(&call, next, 1, 0)])
                };
                if mode != "own " {
                    for unit in &observed.units {
                        assert!(
                            unit.drops()
                                .iter()
                                .all(|drop| !matches!(drop.target(), UnitDropTarget::This(_))),
                            "child end never destroys nonowning this"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn nested_implicit_and_explicit_this_keep_completion_and_reverse_unwind_order() {
    for next in ["0", "return", "break", "continue"] {
        let inner = format!("this.touch(\"inner\", {next})");
        let outer = format!("touch(\"outer\", {inner})");
        let (open, close) = match next {
            "break" => ("loop {", "}"),
            "continue" => ("while (flag) {", "}"),
            _ => ("", ""),
        };
        let text = format!(
            "class Node {{ fun touch(text: String, own next: Int): Int = next; fun run(flag: Boolean): Unit {{ {open} {outer} {close} }} }}"
        );
        let observed = if next == "0" {
            assert_this_ends(&text, &[&inner, &outer], &[])
        } else {
            assert_this_ends(&text, &[], &[(&inner, next, 1, 0), (&outer, next, 2, 1)])
        };
        for unit in &observed.units {
            let receivers: Vec<_> = unit
                .receiver_call_return_ends()
                .iter()
                .map(|end| end.receiver())
                .chain(
                    unit.receiver_call_prefix_ends()
                        .iter()
                        .map(|end| end.receiver()),
                )
                .collect();
            assert_eq!(receivers[0].target(), receivers[1].target());
            assert_ne!(receivers[0].call(), receivers[1].call());
        }
    }
}

#[test]
fn completed_inner_this_is_not_retired_again_by_outer_prefix() {
    for (open, close, jump) in [
        ("", "", "return"),
        ("loop {", "}", "break"),
        ("while (flag) {", "}", "continue"),
    ] {
        let outer = format!("touch(this.ready(), {jump})");
        assert_this_ends(
            &format!(
                "class Node {{ fun ready(): String = \"ready\"; fun touch(text: String, own next: Int): Int = next; fun run(flag: Boolean): Unit {{ {open} {outer} {close} }} }}"
            ),
            &["this.ready()"],
            &[(&outer, jump, 1, 0)],
        );
    }
}

#[test]
fn inner_loop_jump_keeps_outer_this_until_normal_completion() {
    for (loop_start, jump) in [("loop", "break"), ("while (flag)", "continue")] {
        let inner = format!("this.touch(\"inner\", {jump})");
        let outer =
            format!("touch(\"outer\", if (flag) {{ {loop_start} {{ {inner} }}; 0 }} else {{ 0 }})");
        assert_this_ends(
            &format!(
                "class Node {{ fun touch(text: String, own next: Int): Int = next; fun run(flag: Boolean): Unit {{ {outer} }} }}"
            ),
            &[&outer],
            &[(&inner, jump, 1, 0)],
        );
    }
}

#[test]
fn this_shared_completion_keeps_parent_mutation_and_two_phase_activation() {
    let observed = assert_this_ends(
        "class Cell(var value: Int) { fun read(): Int = value; inout fun set(own next: Int): Unit { value = next }; inout fun run(): Unit { set(read()); this.set(2); this.read() } }",
        &["read()", "this.read()"],
        &[],
    );
    for unit in &observed.units {
        let reservations: Vec<_> = unit
            .receiver_facts()
            .iter()
            .filter(|fact| fact.is_receiver_reservation())
            .collect();
        assert_eq!(reservations.len(), 2);
        for fact in reservations {
            assert_eq!(fact.activation_point(), Some(fact.call()));
        }
        assert!(
            unit.drops()
                .iter()
                .all(|drop| !matches!(drop.target(), UnitDropTarget::This(_)))
        );
    }
}

#[test]
fn abandoned_exclusive_this_stays_excluded_while_shared_inner_ends() {
    for (open, close, jump) in [
        ("", "", "return"),
        ("loop {", "}", "break"),
        ("while (flag) {", "}", "continue"),
    ] {
        let inner = format!("this.touch(\"inner\", {jump})");
        let outer = format!("change(\"outer\", {inner})");
        let observed = assert_this_ends(
            &format!(
                "class Node {{ fun touch(text: String, own next: Int): Int = next; inout fun change(text: String, own next: Int): Int = next; inout fun run(flag: Boolean): Unit {{ {open} {outer} {close} }} }}"
            ),
            &[],
            &[(&inner, jump, 1, 0)],
        );
        for unit in &observed.units {
            let reservations: Vec<_> = unit
                .receiver_facts()
                .iter()
                .filter(|fact| fact.is_receiver_reservation())
                .collect();
            assert_eq!(reservations.len(), 1);
            assert_eq!(reservations[0].activation_point(), None);
        }
    }
}

#[test]
fn this_for_prefix_keeps_the_actual_cleanup_order() {
    use lang_frontend::ownership_checking::UnitIterationCleanupAction as Action;
    for jump in ["return", "break", "continue"] {
        let call = format!("this.touch(\"prefix\", {jump})");
        let observed = assert_this_ends(
            &format!(
                "class Node {{ fun touch(text: String, own next: Int): Int = next; fun run(): Unit {{ for (item in listOf(1)) {{ {call} }} }} }}"
            ),
            &[],
            &[(&call, jump, 1, 0)],
        );
        for unit in &observed.units {
            let event = &unit.receiver_call_prefix_ends()[0];
            let actions = unit.iteration_cleanup_at(event.point()).unwrap();
            assert!(
                matches!(&actions[0], Action::EndCallLoan(loan) if loan.call() == event.receiver().call())
            );
            assert!(
                matches!(&actions[1], Action::EndReceiverLoan(receiver) if receiver == event.receiver())
            );
            assert_eq!(
                actions
                    .iter()
                    .filter(|action| matches!(action, Action::EndReceiverLoan(_)))
                    .count(),
                1
            );
            assert_eq!(
                actions
                    .iter()
                    .any(|action| matches!(action, Action::FinishProvider { .. })),
                jump != "continue"
            );
        }
    }
}

#[test]
fn borrowed_results_and_static_self_this_remain_outside_the_slice() {
    for text in [
        "class Node { fun view(source: Node): borrow Node from source = source; fun run(source: Node): Unit { borrow val kept = this.view(source) } }",
        "interface Node { fun read(): Int = 0; fun run(): Unit { this.read() } }",
        "interface Node { fun read(own next: Int): Int = next; fun run(): Unit { read(return) } }",
    ] {
        let observed = assert_this_ends(text, &[], &[]);
        for unit in &observed.units {
            assert!(!unit.receiver_facts().is_empty());
        }
    }
}

#[test]
fn caller_retains_its_one_owner_after_nested_this_reborrow() {
    let observed = observe(
        "class Cell(var value: Int) { fun read(): Int = value; inout fun set(own next: Int): Unit { value = next }; fun wrapper(): Unit { this.read() } }\nfun run(): Unit { var cell = Cell(1); cell.wrapper(); cell.set(2); cell.read() }",
    );
    assert_units_admitted(&observed);
    for (entry, unit) in observed.units.iter().enumerate() {
        assert_eq!(unit.receiver_call_return_ends().len(), 3);
        assert!(unit.receiver_call_prefix_ends().is_empty());
        assert_this_receiver(
            &observed,
            entry,
            unit.receiver_call_return_ends()[0].receiver(),
            "this.read()",
        );
        let caller = unit.receiver_call_return_ends().last().unwrap();
        let UnitReceiverOwnershipTarget::Place(place) = caller.receiver().target() else {
            panic!("caller owner")
        };
        assert!(unit.formed_owned_locals().contains(&place.root()));
        let drops: Vec<_> = unit
            .drops()
            .iter()
            .filter(|drop| drop.target() == UnitDropTarget::Named(place.root()))
            .collect();
        assert_eq!(drops.len(), 1);
        assert_eq!(drops[0].point(), caller.point());
        assert!(
            unit.drops()
                .iter()
                .all(|drop| !matches!(drop.target(), UnitDropTarget::This(_)))
        );
    }
}

#[test]
fn this_normal_end_does_not_permit_moving_the_nonowning_parent() {
    let observed = observe(
        "class Node { fun read(): Int = 1; fun run(): Unit { this.read(); val moved = this } }",
    );
    for unit in &observed.units {
        assert_eq!(unit.diagnostics().len(), 1);
        assert_eq!(unit.diagnostics()[0].code().to_string(), "L0133");
        assert!(unit.receiver_call_return_ends().is_empty());
        assert!(unit.receiver_call_prefix_ends().is_empty());
    }
}

#[test]
fn this_shared_prefix_still_rejects_overlapping_exclusive_reservation() {
    let observed = observe(
        "class Node { fun touch(text: String, own next: Int): Int = next; inout fun change(text: String, own next: Int): Int = next; inout fun run(): Unit { touch(\"outer\", this.change(\"inner\", return)) } }",
    );
    for unit in &observed.units {
        assert_eq!(unit.diagnostics().len(), 1);
        assert_eq!(unit.diagnostics()[0].code().to_string(), "L0135");
        assert!(unit.receiver_call_return_ends().is_empty());
        assert!(unit.receiver_call_prefix_ends().is_empty());
    }
}

#[test]
fn unsupported_borrow_continuation_is_not_admitted_by_this_end_observation() {
    for text in [
        "class Node { fun view(source: Node, own next: Int): borrow Node from source = source; fun run(source: Node): Unit { this.view(source, return) } }",
        "class Node { fun view(own next: Int): borrow Node from this = this; fun run(): Unit { borrow val kept = this.view(0) } }",
        "class Node { fun view(own next: Int): borrow Node from this = this; fun run(): Unit { this.view(return) } }",
    ] {
        let observed = observe(text);
        for unit in &observed.units {
            assert!(!unit.diagnostics().is_empty());
            assert!(
                unit.diagnostics()
                    .iter()
                    .all(|diagnostic| diagnostic.code().to_string() == "L0164")
            );
            assert!(unit.receiver_call_return_ends().is_empty());
            assert!(unit.receiver_call_prefix_ends().is_empty());
        }
    }
}

#[test]
fn conditional_this_call_has_mutually_exclusive_normal_and_prefix_points() {
    let call = "this.read(if (flag) { 0 } else { return })";
    let observed = assert_this_ends(
        &format!(
            "class Node {{ fun read(own next: Int): Int = next; fun run(flag: Boolean): Unit {{ {call} }} }}"
        ),
        &[call],
        &[(call, "return", 0, 0)],
    );
    for unit in &observed.units {
        assert_eq!(
            unit.receiver_call_return_ends()[0].receiver(),
            unit.receiver_call_prefix_ends()[0].receiver()
        );
        assert_ne!(
            unit.receiver_call_return_ends()[0].point(),
            unit.receiver_call_prefix_ends()[0].point()
        );
    }
}
