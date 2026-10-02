use super::*;

#[test]
fn source_loan_prevents_move_replacement_and_exclusive_access() {
    for (parameter, body, expected) in [
        ("own xs: List<Int>", "consume(xs)\nbreak", "xs"),
        ("xs: List<Int>", "consume(xs)\nbreak", "xs"),
        ("inout xs: List<Int>", "replace(&xs)\nbreak", "&"),
        ("inout xs: List<Int>", "xs = listOf(2)\nbreak", "xs"),
    ] {
        let (sources, _, owned) = checked(&format!(
            "fun consume(own xs: List<Int>) {{}}\nfun replace(inout xs: List<Int>) {{}}\nfun run({parameter}) {{ for (_ in xs) {{ {body} }} }}"
        ));
        assert_eq!(
            owned.diagnostics().len(),
            1,
            "{body}: {:?}",
            owned.diagnostics()
        );
        assert_eq!(owned.diagnostics()[0].code().to_string(), "L0135", "{body}");
        assert_eq!(
            sources
                .slice(owned.diagnostics()[0].primary_span())
                .unwrap(),
            expected
        );
        assert!(owned.drops().is_empty(), "invalid plans must not escape");
    }
}

#[test]
fn element_and_component_bindings_cannot_transfer_move_only_owners() {
    for (element, binding, prefix) in [
        ("Node", "n", ""),
        (
            "Pair",
            "(n, _)",
            "value class Pair(val node: Node, val flag: Boolean)",
        ),
    ] {
        let (_, _, owned) = checked(&format!(
            "class Node {{}}\n{prefix}\nfun consume(own n: Node) {{}}\nfun run(xs: List<{element}>) {{ for ({binding} in xs) {{ consume(n)\nbreak }} }}"
        ));
        assert_eq!(owned.diagnostics().len(), 1, "{:?}", owned.diagnostics());
        assert_eq!(owned.diagnostics()[0].code().to_string(), "L0133");
    }
}

#[test]
fn source_owner_is_not_dropped_until_iteration_exits() {
    use lang_frontend::ownership_checking::{CleanupOwnerValue, DropPoint, DropTarget};
    use lang_frontend::parser::Statement;
    for (text, temporary) in [
        ("fun run(own xs: List<Int>) { for (_ in xs) {} }", false),
        ("fun run() { for (_ in listOf(1)) {} }", true),
    ] {
        let (_, parsed, owned) = checked(text);
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        let (statement, source) = parsed
            .ast()
            .statements()
            .iter()
            .find_map(|(id, n)| match n.payload() {
                Statement::For { source, .. } => Some((id, *source)),
                _ => None,
            })
            .unwrap();
        let drops = owned
            .drops()
            .iter()
            .filter(|drop| {
                if temporary {
                    drop.target() == DropTarget::Temporary(source)
                } else {
                    matches!(drop.target(), DropTarget::Named(_))
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(drops.len(), 1);
        assert_eq!(drops[0].point(), DropPoint::LoopExit(statement));
        if temporary {
            let owner = drops[0]
                .owner()
                .expect("direct source has an owner identity");
            assert!(matches!(
                owned.cleanup_conditions().owner_value(owner),
                Some(CleanupOwnerValue::Expression { expression, .. }) if *expression == source
            ));
        }
    }
}

#[test]
fn returns_and_nested_loops_keep_the_outer_provider_loan_active() {
    for body in [
        "return xs",
        "for (_ in xs) { break }\nreturn xs",
        "read(xs)\nreturn xs",
    ] {
        let text = format!(
            "fun read(xs: List<Int>) {{}}\nfun run(own xs: List<Int>): List<Int> {{ for (_ in xs) {{ {body} }} return xs }}"
        );
        let (sources, _, owned) = checked(&text);
        assert_eq!(
            owned.diagnostics().len(),
            1,
            "{body}: {:?}",
            owned.diagnostics()
        );
        assert_eq!(owned.diagnostics()[0].code().to_string(), "L0135");
        let operand_start = text.find("return xs").unwrap() + "return ".len();
        assert_eq!(
            owned.diagnostics()[0].primary_span().start(),
            operand_start,
            "the diagnostic must point to the return operand inside the loop"
        );
        assert_eq!(
            sources.slice(owned.diagnostics()[0].primary_span()),
            Ok("xs")
        );
    }
    let (_, _, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>): List<Int> { for (n in xs) { read(xs)\nfor (m in xs) { val x: Int = m } } return xs }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
}

#[test]
fn borrowed_iteration_binding_preserves_copy_inout_and_capture_rules() {
    for (text, code) in [
        (
            "fun change(inout n: Int) {}\nfun run(xs: List<Int>) { for (n in xs) { change(&n) } }",
            "L0134",
        ),
        (
            "class Node(var field: Int) {}\nfun run(xs: List<Node>) { for (n in xs) { n.field = 2 } }",
            "L0135",
        ),
        (
            "class Node {}\nfun consume(n: Node) {}\nfun run(xs: List<Node>) { for (n in xs) { val capture = move { consume(n) } } }",
            "L0138",
        ),
        (
            "class Node {}\nfun run(xs: List<Node>): Node { for (n in xs) { return n } return Node() }",
            "L0133",
        ),
    ] {
        let (_, _, owned) = checked(text);
        assert_eq!(
            owned.diagnostics().len(),
            1,
            "{text}: {:?}",
            owned.diagnostics()
        );
        assert_eq!(owned.diagnostics()[0].code().to_string(), code);
    }
    let (_, _, owned) =
        checked("fun run(xs: List<Int>): Int { for (n in xs) { return n } return 0 }");
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
}

#[test]
fn copyable_fields_are_reads_through_the_borrowed_element() {
    for declaration in [
        "value class Point(val x: Int)",
        "class Point(val x: Int) {}",
    ] {
        let (_, _, owned) = checked(&format!(
            "{declaration}\nfun run(xs: List<Point>) {{ for (p in xs) {{ val n: Int = p.x }} }}"
        ));
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    }
}

#[test]
fn source_category_plans_preserve_place_and_owner_capabilities() {
    use lang_frontend::ownership_checking::{
        DropTarget, IterationCleanupAction as Action, LoanTarget,
    };
    for (parameter, source, fields, owns) in [
        ("own xs: List<Int>", "xs", 0, true),
        ("xs: List<Int>", "xs", 0, false),
        ("inout xs: List<Int>", "xs", 0, false),
        ("own h: Holder", "h.xs", 1, true),
        ("h: Holder", "h.xs", 1, false),
    ] {
        let (_, _, owned) = checked(&format!(
            "class Holder(val xs: List<Int>) {{}}\nfun run({parameter}) {{ for (_ in {source}) {{}} }}"
        ));
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        let plan = &owned.iterations()[0];
        let LoanTarget::Place(place) = plan.source() else {
            panic!("expected source place")
        };
        assert_eq!(place.fields().len(), fields);
        let drops = plan.exits().iter().flat_map(|exit| exit.actions()).filter(|action| matches!(action, Action::Drop(fact) if fact.target() == DropTarget::Named(place.root()))).count();
        assert_eq!(drops, usize::from(owns), "{parameter}");
    }
}

#[test]
fn provider_loan_ends_before_an_inout_source_is_replaced_after_the_loop() {
    use lang_frontend::ownership_checking::{
        IterationCleanupAction as Action, IterationExitKind, LoanTarget,
    };

    for (parameter, source, replacement) in [
        ("inout xs: List<Int>", "xs", "xs = listOf(2)"),
        ("inout h: Holder", "h.xs", "h.xs = listOf(2)"),
    ] {
        let (_, _, owned) = checked(&format!(
            "class Holder(var xs: List<Int>) {{}}\nfun run({parameter}) {{ for (_ in {source}) {{ break }}\n{replacement} }}"
        ));
        assert!(
            owned.diagnostics().is_empty(),
            "{source}: {:?}",
            owned.diagnostics()
        );
        let plan = &owned.iterations()[0];
        assert!(matches!(plan.source(), LoanTarget::Place(_)));
        assert_eq!(
            plan.exits()
                .iter()
                .filter(|exit| matches!(exit.kind(), IterationExitKind::Exhaustion))
                .count(),
            1
        );
        assert_eq!(
            plan.exits()
                .iter()
                .filter(|exit| matches!(exit.kind(), IterationExitKind::Break(_)))
                .count(),
            1
        );
        let statement = plan.descriptor().statement();
        for exit in plan.exits().iter().filter(|exit| {
            matches!(
                exit.kind(),
                IterationExitKind::Exhaustion | IterationExitKind::Break(_)
            )
        }) {
            let actions = exit.actions();
            let finish = actions
                .iter()
                .position(|action| matches!(action, Action::FinishProvider(id) if *id == statement))
                .unwrap();
            let end = actions
                .iter()
                .position(|action| matches!(action, Action::EndSource(id) if *id == statement))
                .unwrap();
            assert_eq!(end, finish + 1, "{source}: {:?}", exit.kind());
            assert_eq!(
                actions
                    .iter()
                    .filter(|action| matches!(action, Action::FinishProvider(_)))
                    .count(),
                1
            );
            assert_eq!(
                actions
                    .iter()
                    .filter(|action| matches!(action, Action::EndSource(_)))
                    .count(),
                1
            );
        }
    }
}

#[test]
fn explicit_this_field_source_has_the_same_provider_loan_as_a_bare_field() {
    use lang_frontend::ownership_checking::LoanTarget;

    for (source, target) in [
        ("xs", "xs"),
        ("this.xs", "xs"),
        ("(this).xs", "xs"),
        ("xs", "this.xs"),
        ("this.xs", "this.xs"),
    ] {
        let (_, _, owned) = checked(&format!(
            "class Holder(var xs: List<Int>) {{ inout fun scan(): Unit {{ for (_ in {source}) {{ {target} = listOf(2) }} }} }}"
        ));
        assert_eq!(
            owned.diagnostics().len(),
            1,
            "{source} / {target}: {:?}",
            owned.diagnostics()
        );
        assert_eq!(owned.diagnostics()[0].code().to_string(), "L0135");
    }

    let (_, _, owned) = checked(
        "class Holder(var xs: List<Int>) { inout fun scan(): Unit { for (_ in this.xs) {} } }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(matches!(
        owned.iterations()[0].source(),
        LoanTarget::Place(_)
    ));
}

#[test]
fn explicit_this_field_does_not_upgrade_a_borrowed_receiver_for_inout_calls() {
    for mode in ["borrow", "own"] {
        let (_, _, owned) = checked(&format!(
            "class Cell(var n: Int) {{ inout fun set(): Unit {{ n = 1 }} }}\nclass Holder(val cell: Cell) {{ {mode} fun bad(): Unit {{ this.cell.set() }} }}"
        ));
        assert_eq!(
            owned.diagnostics().len(),
            1,
            "{mode}: {:?}",
            owned.diagnostics()
        );
        assert_eq!(owned.diagnostics()[0].code().to_string(), "L0134");
    }

    let (_, _, owned) = checked(
        "class Cell(var n: Int) { inout fun set(): Unit { n = 1 } }\nclass Holder(val cell: Cell) { inout fun good(): Unit { this.cell.set() } }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
}

#[test]
fn explicit_this_field_respects_receiver_capability_for_writes_and_captures() {
    for (mode, target) in [("borrow", "xs"), ("borrow", "this.xs"), ("own", "this.xs")] {
        let (_, _, owned) = checked(&format!(
            "class Holder(var xs: List<Int>) {{ {mode} fun bad(): Unit {{ {target} = listOf(2) }} }}"
        ));
        assert_eq!(
            owned.diagnostics().len(),
            1,
            "{mode} / {target}: {:?}",
            owned.diagnostics()
        );
        assert_eq!(owned.diagnostics()[0].code().to_string(), "L0135");
    }

    let (_, _, owned) = checked(
        "class Holder(var xs: List<Int>) { inout fun good(): Unit { this.xs = listOf(2) } }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());

    let (_, _, owned) = checked(
        "class Cell(var n: Int) { inout fun set(): Unit { n = 1 } }\nclass Holder(val cell: Cell) { inout fun bad(): Unit { val f: () -> Unit = { this.cell.set() } } }",
    );
    assert_eq!(owned.diagnostics().len(), 1, "{:?}", owned.diagnostics());
    assert_eq!(owned.diagnostics()[0].code().to_string(), "L0134");

    let (_, _, owned) = checked(
        "class Cell(var n: Int) { inout fun set(): Unit { n = 1 }\ninout fun bad(): Unit { val f: () -> Unit = { set() } } }",
    );
    assert_eq!(owned.diagnostics().len(), 1, "{:?}", owned.diagnostics());
    assert_eq!(owned.diagnostics()[0].code().to_string(), "L0134");
}

#[test]
fn source_reuse_and_moved_entry_follow_the_original_owner() {
    use lang_frontend::ownership_checking::{
        DropTarget, IterationCleanupAction as Action, LoanTarget,
    };
    let (_, _, good) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>) { for (_ in xs) { break } read(xs) }",
    );
    let plan = &good.iterations()[0];
    let LoanTarget::Place(place) = plan.source() else {
        panic!("expected place")
    };
    assert!(!plan.exits().iter().flat_map(|exit| exit.actions()).any(|action| matches!(action, Action::Drop(fact) if fact.target() == DropTarget::Named(place.root()))));
    let (_, _, bad) = checked(
        "fun consume(own xs: List<Int>) {}\nfun run(own xs: List<Int>) { consume(xs)\nfor (_ in xs) {} }",
    );
    assert_eq!(bad.diagnostics().len(), 1);
    assert_eq!(bad.diagnostics()[0].code().to_string(), "L0131");
    assert!(bad.iterations().is_empty());
}

#[test]
fn component_bindings_end_in_reverse_order_without_owning_element_drops() {
    use lang_frontend::ownership_checking::{
        IterationCleanupAction as Action, IterationExitKind, OwnershipBindingKind,
    };
    let (_, _, owned) = checked(
        "class Node {}\nvalue class Parts(val first: Node, val flag: Int, val second: Node)\nfun run(xs: List<Parts>) { for ((a, _, b) in xs) { continue } }",
    );
    let plan = &owned.iterations()[0];
    assert_eq!(plan.bindings().len(), 2);
    assert!(
        plan.bindings()
            .iter()
            .all(|binding| binding.kind() == OwnershipBindingKind::Shared)
    );
    let exit = plan
        .exits()
        .iter()
        .find(|exit| matches!(exit.kind(), IterationExitKind::Continue(_)))
        .unwrap();
    let ended = exit
        .actions()
        .iter()
        .filter_map(|action| match action {
            Action::EndBinding { symbol, .. } => Some(*symbol),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        ended,
        plan.bindings()
            .iter()
            .rev()
            .map(|binding| binding.symbol())
            .collect::<Vec<_>>()
    );
    assert!(
        !exit
            .actions()
            .iter()
            .any(|action| matches!(action, Action::Drop(_)))
    );
}

#[test]
fn indexed_temporary_source_keeps_the_backing_container_owner() {
    use lang_frontend::{
        ownership_checking::{CleanupOwnerValue, DropPoint, DropTarget},
        parser::{Expression, Statement},
    };
    for jump in ["", "break", "continue", "return"] {
        let (_, parsed, owned) = checked(&format!(
            "fun run() {{ for (_ in listOf(listOf(1))[0]) {{ {jump} }} }}"
        ));
        assert!(
            owned.diagnostics().is_empty(),
            "{jump}: {:?}",
            owned.diagnostics()
        );
        let (statement, source) = parsed
            .ast()
            .statements()
            .iter()
            .find_map(|(id, node)| match node.payload() {
                Statement::For { source, .. } => Some((id, *source)),
                _ => None,
            })
            .unwrap();
        let Expression::Index { receiver, .. } =
            parsed.ast().expressions().get(source).unwrap().payload()
        else {
            panic!("index source")
        };
        let drops = owned
            .drops()
            .iter()
            .filter(|fact| matches!(fact.target(), DropTarget::Temporary(_)))
            .collect::<Vec<_>>();
        assert!(
            drops
                .iter()
                .all(|fact| fact.target() == DropTarget::Temporary(*receiver)),
            "{jump}: {drops:?}"
        );
        let owner = drops[0]
            .owner()
            .expect("the backing container has an owner identity");
        assert!(drops.iter().all(|fact| fact.owner() == Some(owner)));
        assert!(matches!(
            owned.cleanup_conditions().owner_value(owner),
            Some(CleanupOwnerValue::Expression { expression, .. }) if *expression == *receiver
        ));
        assert_eq!(
            drops
                .iter()
                .filter(|fact| fact.point() == DropPoint::LoopExit(statement))
                .count(),
            1
        );
        assert_eq!(
            drops.len(),
            if matches!(jump, "break" | "return") {
                2
            } else {
                1
            }
        );
        assert_eq!(owned.iterations().len(), 1);
    }
}

#[test]
fn source_index_exit_cleans_only_the_evaluated_backing_owner() {
    use lang_frontend::{
        ownership_checking::{CleanupOwnerValue, DropPoint, DropTarget},
        parser::{Expression, Statement},
    };
    let (_, parsed, owned) = checked(
        "fun run(flag: Boolean) { for (_ in listOf(listOf(1))[if (flag) return else 0]) {} }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let jump = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| matches!(node.payload(), Expression::Return { .. }).then_some(id))
        .unwrap();
    let owner = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(_, node)| match node.payload() {
            Expression::Index { receiver, .. } => Some(*receiver),
            _ => None,
        })
        .unwrap();
    let drops = owned
        .drops()
        .iter()
        .filter(|fact| fact.point() == DropPoint::ControlTransfer(jump))
        .collect::<Vec<_>>();
    assert_eq!(drops.len(), 1);
    assert_eq!(drops[0].target(), DropTarget::Temporary(owner));
    let value = drops[0].owner().expect("pending backing owner identity");
    assert!(matches!(
        owned.cleanup_conditions().owner_value(value),
        Some(CleanupOwnerValue::Expression { expression, .. }) if *expression == owner
    ));
    let statement = parsed
        .ast()
        .statements()
        .iter()
        .find_map(|(id, node)| matches!(node.payload(), Statement::For { .. }).then_some(id))
        .unwrap();
    let normal = owned
        .drops()
        .iter()
        .find(|fact| {
            fact.point() == DropPoint::LoopExit(statement)
                && fact.target() == DropTarget::Temporary(owner)
        })
        .unwrap();
    assert_eq!(
        normal.owner(),
        Some(value),
        "both exits own the same backing definition"
    );
    let (_, _, abort) = checked("fun run() { for (_ in listOf(listOf(1))[error(\"stop\")]) {} }");
    assert!(abort.diagnostics().is_empty());
    assert!(abort.drops().is_empty());
    assert!(abort.iterations().is_empty());
}

#[test]
fn source_borrow_conflicts_with_an_earlier_exclusive_argument() {
    let (_, _, owned) = checked(
        "fun use(inout xs: List<Int>, n: Int) {}\nfun run(inout xs: List<Int>, flag: Boolean) { use(&xs, if (flag) { for (_ in xs) {}\n0 } else 0) }",
    );
    assert_eq!(owned.diagnostics().len(), 1, "{:?}", owned.diagnostics());
    assert_eq!(owned.diagnostics()[0].code().to_string(), "L0135");
    assert!(owned.iterations().is_empty());
}

#[test]
fn multi_index_source_keeps_the_original_temporary_owner() {
    use lang_frontend::{
        ownership_checking::{CleanupOwnerValue, DropTarget, LoanTarget},
        parser::{Expression, Statement},
    };
    for index in ["0", "if (flag) return else 0"] {
        let (sources, parsed, owned) = checked(&format!(
            "fun run(flag: Boolean) {{ for (_ in listOf(listOf(listOf(1)))[0][{index}]) {{}} }}"
        ));
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        let owner = parsed
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, node)| {
                (sources.slice(node.span()).unwrap() == "listOf(listOf(listOf(1)))"
                    && matches!(node.payload(), Expression::Call { .. }))
                .then_some(id)
            })
            .unwrap();
        let statement = parsed
            .ast()
            .statements()
            .iter()
            .find_map(|(id, node)| matches!(node.payload(), Statement::For { .. }).then_some(id))
            .unwrap();
        assert_eq!(
            owned.iteration(statement).unwrap().source(),
            &LoanTarget::Temporary(owner)
        );
        let drops = owned
            .drops()
            .iter()
            .filter(|fact| fact.target() == DropTarget::Temporary(owner))
            .collect::<Vec<_>>();
        assert_eq!(drops.len(), if index == "0" { 1 } else { 2 });
        let definition = drops[0].owner();
        for fact in drops {
            let value = fact
                .owner()
                .expect("every backing drop identifies its owner");
            assert_eq!(
                Some(value),
                definition,
                "all exits share one backing definition"
            );
            assert!(matches!(
                owned.cleanup_conditions().owner_value(value),
                Some(CleanupOwnerValue::Expression { expression, .. }) if *expression == owner
            ));
        }
    }
}

#[test]
fn multi_index_named_source_blocks_owner_move_and_keeps_it_alive() {
    let (_, _, owned) = checked(
        "fun consume(own xs: List<List<List<Int>>>) {}\nfun run(own xs: List<List<List<Int>>>) { for (_ in xs[0][0]) { consume(xs)\nbreak } }",
    );
    assert_eq!(owned.diagnostics().len(), 1, "{:?}", owned.diagnostics());
    assert_eq!(owned.diagnostics()[0].code().to_string(), "L0135");
    assert!(owned.iterations().is_empty());
}

#[test]
fn multi_index_places_preserve_prefix_aliases_and_named_owner_cleanup() {
    use lang_frontend::ownership_checking::{
        DropPoint, DropTarget, ElementIndexIdentity, LoanTarget,
    };
    let (_, _, owned) = checked(
        "fun run(own xs: List<List<List<Int>>>, i: Int) { for (_ in xs[0]) {}\nfor (_ in xs[0][0]) {}\nfor (_ in xs[0][1]) {}\nfor (_ in xs[i][0]) {} }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let plans = owned.iterations();
    let places = plans
        .iter()
        .map(|plan| match plan.source() {
            LoanTarget::Place(place) => place,
            LoanTarget::Temporary(_) | LoanTarget::This(_) => panic!("named source lost its place"),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        places[1].elements(),
        &[
            ElementIndexIdentity::Known(0),
            ElementIndexIdentity::Known(0)
        ]
    );
    assert!(places[0].overlaps(places[1]));
    assert!(!places[1].overlaps(places[2]));
    assert!(places[1].overlaps(places[3]));
    let drops = owned
        .drops()
        .iter()
        .filter(|fact| fact.target() == DropTarget::Named(places[0].root()))
        .collect::<Vec<_>>();
    assert_eq!(drops.len(), 1);
    assert!(matches!(drops[0].point(), DropPoint::LoopExit(_)));
}
