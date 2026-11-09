//! Named-root field paths retain the actual Shared receiver token and aggregate owner.
use super::receiver_return::NODE;
use super::*;
use lang_frontend::{
    ownership_checking::{
        UnitReceiverOwnershipFact, UnitReceiverOwnershipKind, UnitReceiverOwnershipTarget,
    },
    type_checking::UnitCallReceiverOrigin,
};

struct FieldCall<'a> {
    call: &'a str,
    operand: &'a str,
    path: &'a [&'a str],
}
fn field<'a>(call: &'a str, operand: &'a str, path: &'a [&'a str]) -> FieldCall<'a> {
    FieldCall {
        call,
        operand,
        path,
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
fn original_field_normal_sources_now_observe_the_shared_reborrow_end() {
    // Preserve the exact owned and Inout sources from the former exclusion tests.
    for mode in ["own ", "inout "] {
        assert_field_ends(
            &format!(
                "{NODE}class Holder(val node: Node)\nfun run({mode}holder: Holder): Unit {{ holder.node.touch(\"field\", 0) }}"
            ),
            &[field(
                "holder.node.touch(\"field\", 0)",
                "holder.node",
                &["holder", "node"],
            )],
            &[],
        );
    }
}

#[test]
fn original_field_prefix_sources_now_observe_the_shared_reborrow_end() {
    for mode in ["own ", "inout "] {
        assert_field_ends(
            &format!(
                "{NODE}class Holder(val node: Node)\nfun run({mode}holder: Holder): Unit {{ holder.node.touch(\"field\", return) }}"
            ),
            &[],
            &[(
                field(
                    "holder.node.touch(\"field\", return)",
                    "holder.node",
                    &["holder", "node"],
                ),
                "return",
                1,
                0,
            )],
        );
    }
}

#[test]
fn grouped_nested_fields_preserve_every_symbol_and_nonowning_root() {
    for mode in ["own ", "", "borrow ", "inout "] {
        for (open, close, next) in [
            ("", "", "0"),
            ("", "", "return"),
            ("loop {", "}", "break"),
            ("while (flag) {", "}", "continue"),
        ] {
            let operand = "((holder).inner).node";
            let call = format!("{operand}.touch(\"nested\", {next})");
            let text = format!(
                "{NODE}class Inner(val node: Node)\nclass Holder(val inner: Inner)\nfun run({mode}holder: Holder, flag: Boolean): Unit {{ {open} {call} {close} }}"
            );
            let expected = field(&call, operand, &["holder", "inner", "node"]);
            let observed = if next == "0" {
                assert_field_ends(&text, &[expected], &[])
            } else {
                assert_field_ends(&text, &[], &[(expected, next, 1, 0)])
            };
            if mode != "own " {
                for unit in &observed.units {
                    let receiver = unit
                        .receiver_call_return_ends()
                        .first()
                        .map(|e| e.receiver())
                        .or_else(|| {
                            unit.receiver_call_prefix_ends()
                                .first()
                                .map(|e| e.receiver())
                        })
                        .unwrap();
                    let UnitReceiverOwnershipTarget::Place(place) = receiver.target() else {
                        panic!("field path")
                    };
                    assert!(
                        unit.drops()
                            .iter()
                            .all(|drop| drop.target() != UnitDropTarget::Named(place.root()))
                    );
                }
            }
        }
    }
}

#[test]
fn local_aggregate_keeps_one_owner_and_never_drops_the_borrowed_field_separately() {
    for next in ["0", "return"] {
        let call = format!("holder.node.touch(\"local\", {next})");
        let text = format!(
            "{NODE}class Holder(val node: Node)\nfun run(): Unit {{ val holder = Holder(Node()); {call} }}"
        );
        let expected = field(&call, "holder.node", &["holder", "node"]);
        let observed = if next == "0" {
            assert_field_ends(&text, &[expected], &[])
        } else {
            assert_field_ends(&text, &[], &[(expected, next, 1, 0)])
        };
        for unit in &observed.units {
            let (receiver, point) = if next == "0" {
                let e = &unit.receiver_call_return_ends()[0];
                (e.receiver(), e.point())
            } else {
                let e = &unit.receiver_call_prefix_ends()[0];
                (e.receiver(), e.point())
            };
            let UnitReceiverOwnershipTarget::Place(place) = receiver.target() else {
                panic!("field")
            };
            assert!(unit.formed_owned_locals().contains(&place.root()));
            let drops: Vec<_> = unit
                .drops()
                .iter()
                .filter(|d| d.target() == UnitDropTarget::Named(place.root()))
                .collect();
            assert_eq!(drops.len(), 1);
            assert_eq!(drops[0].point(), point);
            for field in place.fields() {
                assert!(
                    unit.drops()
                        .iter()
                        .all(|d| d.target() != UnitDropTarget::Named(*field))
                );
            }
        }
    }
}

#[test]
fn repeated_field_tokens_keep_normal_order_and_reverse_prefix_order() {
    for next in ["0", "return", "break", "continue"] {
        let inner = format!("holder.node.touch(\"inner\", {next})");
        let outer = format!("holder.node.touch(\"outer\", {inner})");
        let (open, close) = match next {
            "break" => ("loop {", "}"),
            "continue" => ("while (flag) {", "}"),
            _ => ("", ""),
        };
        let text = format!(
            "{NODE}class Holder(val node: Node)\nfun run(holder: Holder, flag: Boolean): Unit {{ {open} {outer} {close} }}"
        );
        let first = field(&inner, "holder.node", &["holder", "node"]);
        let second = field(&outer, "holder.node", &["holder", "node"]);
        let observed = if next == "0" {
            assert_field_ends(&text, &[first, second], &[])
        } else {
            assert_field_ends(&text, &[], &[(first, next, 1, 0), (second, next, 2, 1)])
        };
        for unit in &observed.units {
            let receivers: Vec<_> = unit
                .receiver_call_return_ends()
                .iter()
                .map(|e| e.receiver())
                .chain(
                    unit.receiver_call_prefix_ends()
                        .iter()
                        .map(|e| e.receiver()),
                )
                .collect();
            assert_eq!(receivers[0].target(), receivers[1].target());
            assert_ne!(receivers[0].call(), receivers[1].call());
        }
    }
}

#[test]
fn sibling_fields_keep_distinct_paths_while_sharing_the_backing_root() {
    let inner = "holder.right.touch(\"inner\", return)";
    let outer = format!("holder.left.touch(\"outer\", {inner})");
    let observed = assert_field_ends(
        &format!(
            "{NODE}class Holder(val left: Node, val right: Node)\nfun run(holder: Holder): Unit {{ {outer} }}"
        ),
        &[],
        &[
            (
                field(inner, "holder.right", &["holder", "right"]),
                "return",
                1,
                0,
            ),
            (
                field(&outer, "holder.left", &["holder", "left"]),
                "return",
                2,
                1,
            ),
        ],
    );
    for unit in &observed.units {
        let paths: Vec<_> = unit
            .receiver_call_prefix_ends()
            .iter()
            .map(|e| match e.receiver().target() {
                UnitReceiverOwnershipTarget::Place(p) => p,
                _ => panic!("path"),
            })
            .collect();
        assert_eq!(paths[0].root(), paths[1].root());
        assert_ne!(paths[0].fields(), paths[1].fields());
    }
}

#[test]
fn completed_inner_field_is_not_ended_twice_when_outer_prefix_exits() {
    for (open, close, next) in [
        ("", "", "return"),
        ("loop {", "}", "break"),
        ("while (flag) {", "}", "continue"),
    ] {
        let outer = format!("holder.node.touch(holder.node.ready(), {next})");
        assert_field_ends(
            &format!(
                "class Node {{ fun ready(): String = \"ready\"; fun touch(text: String, own next: Int): Int = next }}\nclass Holder(val node: Node)\nfun run(holder: Holder, flag: Boolean): Unit {{ {open} {outer} {close} }}"
            ),
            &[field(
                "holder.node.ready()",
                "holder.node",
                &["holder", "node"],
            )],
            &[(
                field(&outer, "holder.node", &["holder", "node"]),
                next,
                1,
                0,
            )],
        );
    }
}

#[test]
fn inner_loop_jump_does_not_retire_the_outer_field_token() {
    for (loop_start, jump) in [("loop", "break"), ("while (flag)", "continue")] {
        let inner = format!("holder.node.touch(\"inner\", {jump})");
        let outer = format!(
            "holder.node.touch(\"outer\", if (flag) {{ {loop_start} {{ {inner} }}; 0 }} else {{ 0 }})"
        );
        assert_field_ends(
            &format!(
                "{NODE}class Holder(val node: Node)\nfun run(holder: Holder, flag: Boolean): Unit {{ {outer} }}"
            ),
            &[field(&outer, "holder.node", &["holder", "node"])],
            &[(
                field(&inner, "holder.node", &["holder", "node"]),
                jump,
                1,
                0,
            )],
        );
    }
}

#[test]
fn finished_field_reborrow_allows_replacement_without_dropping_the_inout_parent() {
    let observed = assert_field_ends(
        &format!(
            "{NODE}class Holder(var node: Node)\nfun run(inout holder: Holder): Unit {{ holder.node.touch(\"before\", 0); holder.node = Node(); holder.node.touch(\"after\", 0) }}"
        ),
        &[
            field(
                "holder.node.touch(\"before\", 0)",
                "holder.node",
                &["holder", "node"],
            ),
            field(
                "holder.node.touch(\"after\", 0)",
                "holder.node",
                &["holder", "node"],
            ),
        ],
        &[],
    );
    for unit in &observed.units {
        let UnitReceiverOwnershipTarget::Place(place) =
            unit.receiver_call_return_ends()[0].receiver().target()
        else {
            panic!("field")
        };
        assert!(
            unit.drops()
                .iter()
                .all(|d| d.target() != UnitDropTarget::Named(place.root()))
        );
        let replaced: Vec<_> = unit.drops().iter().filter(|d| matches!(d.target(), UnitDropTarget::ReplacedField { field, .. } if field == place.fields()[0])).collect();
        assert_eq!(replaced.len(), 1);
        assert!(matches!(
            replaced[0].point(),
            UnitDropPoint::BeforeReplacement(_)
        ));
    }
}

#[test]
fn excluded_exclusive_sibling_counts_in_real_prefix_order() {
    for next in ["0", "return"] {
        let outer = format!("holder.left.touch(\"outer\", holder.right.change(\"inner\", {next}))");
        let text = format!(
            "class Node {{ fun touch(text: String, own next: Int): Int = next; inout fun change(text: String, own next: Int): Int = next }}\nclass Holder(val left: Node, val right: Node)\nfun run(inout holder: Holder): Unit {{ {outer} }}"
        );
        let expected = field(&outer, "holder.left", &["holder", "left"]);
        let observed = if next == "0" {
            assert_field_ends(&text, &[expected], &[])
        } else {
            assert_field_ends(&text, &[], &[(expected, next, 2, 1)])
        };
        for unit in &observed.units {
            let reservations: Vec<_> = unit
                .receiver_facts()
                .iter()
                .filter(|f| f.is_receiver_reservation())
                .collect();
            assert_eq!(reservations.len(), 1);
            assert_eq!(
                reservations[0].activation_point(),
                (next == "0").then_some(reservations[0].call())
            );
        }
    }
}

#[test]
fn same_field_exclusive_conflict_is_not_hidden_by_end_observation() {
    let observed = observe(
        "class Node { fun touch(text: String, own next: Int): Int = next; inout fun change(own next: Int): Int = next }\nclass Holder(val node: Node)\nfun run(inout holder: Holder): Unit { holder.node.touch(\"outer\", holder.node.change(0)) }",
    );
    for unit in &observed.units {
        assert_eq!(unit.diagnostics().len(), 1);
        assert_eq!(unit.diagnostics()[0].code().to_string(), "L0135");
        assert!(unit.receiver_call_return_ends().is_empty());
        assert!(unit.receiver_call_prefix_ends().is_empty());
    }
}

#[test]
fn temporary_this_index_provider_and_binding_paths_remain_excluded() {
    for next in ["0", "return"] {
        for text in [
            format!("{NODE}fun run(): Unit {{ Node().touch(\"temporary\", {next}) }}"),
            format!(
                "{NODE}class Holder(val node: Node) {{ fun run(): Unit {{ this.node.touch(\"this-field\", {next}) }} }}"
            ),
            format!(
                "{NODE}class Holder(val node: Node) {{ fun run(): Unit {{ node.touch(\"implicit-field\", {next}) }} }}"
            ),
            format!(
                "{NODE}class Holder(val nodes: List<Node>)\nfun run(holder: Holder): Unit {{ holder.nodes[0].touch(\"index\", {next}) }}"
            ),
            format!(
                "{NODE}class Holder(val node: Node)\nfun run(holders: List<Holder>): Unit {{ for (holder in holders) {{ holder.node.touch(\"provider\", {next}) }} }}"
            ),
            format!(
                "{NODE}class Holder(val node: Node)\nfun alias(source: Holder): borrow Holder from source = source\nfun run(holder: Holder): Unit {{ borrow val borrowed = alias(holder); borrowed.node.touch(\"binding\", {next}) }}"
            ),
        ] {
            let observed = assert_field_ends(&text, &[], &[]);
            for unit in &observed.units {
                assert!(!unit.receiver_facts().is_empty());
            }
        }
    }
}

#[test]
fn field_end_does_not_admit_partial_move_of_the_aggregate() {
    let observed = observe(&format!(
        "{NODE}class Holder(val node: Node)\nfun consume(own node: Node): Unit {{}}\nfun run(own holder: Holder): Unit {{ holder.node.touch(\"before\", 0); consume(holder.node) }}"
    ));
    for unit in &observed.units {
        assert_eq!(unit.diagnostics().len(), 1);
        assert_eq!(unit.diagnostics()[0].code().to_string(), "L0132");
        assert!(unit.receiver_call_return_ends().is_empty());
        assert!(unit.receiver_call_prefix_ends().is_empty());
    }
}

#[test]
fn field_for_prefix_uses_actual_cleanup_before_provider_finish() {
    use lang_frontend::ownership_checking::UnitIterationCleanupAction as Action;
    for jump in ["return", "break", "continue"] {
        let call = format!("holder.node.touch(\"prefix\", {jump})");
        let observed = assert_field_ends(
            &format!(
                "{NODE}class Holder(val node: Node)\nfun run(holder: Holder): Unit {{ for (item in listOf(1)) {{ {call} }} }}"
            ),
            &[],
            &[(field(&call, "holder.node", &["holder", "node"]), jump, 1, 0)],
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
                    .any(|a| matches!(a, Action::FinishProvider { .. })),
                jump != "continue"
            );
        }
    }
}

#[test]
fn conditional_field_call_keeps_mutually_exclusive_normal_and_prefix_points() {
    let call = "holder.node.touch(\"branch\", if (flag) { 0 } else { return })";
    let observed = assert_field_ends(
        &format!(
            "{NODE}class Holder(val node: Node)\nfun run(holder: Holder, flag: Boolean): Unit {{ {call} }}"
        ),
        &[field(call, "holder.node", &["holder", "node"])],
        &[(
            field(call, "holder.node", &["holder", "node"]),
            "return",
            1,
            0,
        )],
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

fn assert_unformed_field_source(text: &str, operand: &str, type_rejected: bool) {
    use lang_frontend::{
        ownership_checking::OwnershipCheckingError, type_checking::FinalTypeAdmissionError,
    };
    let mut sources = SourceMap::new();
    let source = sources.add_source("call_ends.ko", text).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(parsed.diagnostics().is_empty());
    let expression = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == operand).then_some(id))
        .unwrap();
    let (environment, types) = standard_environments();
    for input in [
        SourceUnitInput::new("root", "call_ends.ko", source, &parsed),
        SourceUnitInput::memory_document(source, &parsed),
    ] {
        let inputs = [input];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty());
        let completion = complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed);
        if type_rejected {
            let Err(FinalTypeAdmissionError::Diagnostics(diagnostics)) = completion else {
                panic!("safe field member has no final type")
            };
            assert_eq!(diagnostics.len(), 1);
            assert_eq!(diagnostics[0].code().to_string(), "L0165");
            assert_eq!(
                sources.slice(diagnostics[0].primary_span()).unwrap(),
                operand
            );
        } else {
            completion.unwrap();
            let typed = typed.validate().unwrap();
            let result =
                check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed);
            assert!(
                matches!(result, Err(OwnershipCheckingError::InvalidUnitArgumentPlace { source_unit, expression: invalid }) if source_unit == names.names().index().source_units()[0].id().index() && invalid == expression.index()),
                "{result:?}"
            );
        }
    }
}

#[test]
fn temporary_field_receiver_keeps_its_existing_unformed_place_error() {
    for next in ["0", "return"] {
        // Preserve this exact failed probe; it is not an admitted empty-table oracle.
        assert_unformed_field_source(
            &format!(
                "{NODE}class Holder(val node: Node)\nfun run(): Unit {{ Holder(Node()).node.touch(\"temporary\", {next}) }}"
            ),
            "Holder(Node()).node",
            false,
        );
    }
}

#[test]
fn safe_field_receiver_keeps_its_existing_final_type_rejection() {
    for next in ["0", "return"] {
        assert_unformed_field_source(
            &format!(
                "{NODE}class Holder(val node: Node)\nfun run(holder: Holder?): Unit {{ holder?.node?.touch(\"safe\", {next}) }}"
            ),
            "holder?.node",
            true,
        );
    }
}

#[test]
fn original_field_loop_sources_now_observe_their_shared_prefix_end() {
    for (loop_start, jump) in [("loop", "break"), ("while (flag)", "continue")] {
        // Preserve both exact original receiver_cfg_prefix.rs sources.
        let text = format!(
            "{NODE}class Holder(val node: Node)\nfun run(own holder: Holder, flag: Boolean): Unit {{ {loop_start} {{ holder.node.touch(\"field\", {jump}) }} }}"
        );
        let call = format!("holder.node.touch(\"field\", {jump})");
        assert_field_ends(
            &text,
            &[],
            &[(field(&call, "holder.node", &["holder", "node"]), jump, 1, 0)],
        );
    }
}
