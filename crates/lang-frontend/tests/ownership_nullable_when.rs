//! SPEC-0203 nullable branch views preserve the original owner's capabilities.

use lang_frontend::{
    name_resolution::resolve_names,
    ownership_checking::{
        CompilationUnitOwnership, DropPoint, DropTarget, LoanKind, NullableWhenBranchOutcome,
        NullableWhenExtractionKind, OwnershipBindingKind, OwnershipCheckedFile, UnitDropPoint,
        UnitDropTarget, UnitLoanTarget, check_ownership,
    },
    parser::{Expression, ParsedFile},
    source::SourceMap,
    type_checking::{UnitExpressionId, UnitNullableWhenDomain, check_types, standard_environments},
};

#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

#[path = "support/nullable_ownership_unit.rs"]
mod nullable_unit;

fn checked(text: &str) -> OwnershipCheckedFile {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source("nullable-ownership.ko", text)
        .expect("source");
    let parsed = parser_test_assertions::parse_file_twice(&sources, source, "nullable ownership");
    assert!(
        parsed.diagnostics().is_empty(),
        "{text}: {:?}",
        parsed.diagnostics()
    );
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &parsed, &environment).expect("names");
    assert!(
        names.diagnostics().is_empty(),
        "{text}: {:?}",
        names.diagnostics()
    );
    let typed = check_types(&sources, &parsed, &names, &types).expect("types");
    assert!(
        typed.diagnostics().is_empty(),
        "{text}: {:?}",
        typed.diagnostics()
    );
    check_ownership(&sources, &parsed, &names, &typed).expect("ownership")
}

fn checked_nullable_unit(text: &str) -> [CompilationUnitOwnership; 2] {
    [false, true].map(|rooted| {
        let (_, _, owned) = nullable_unit::checked(
            text,
            "nullable-ownership.ko",
            rooted,
            standard_environments(),
        );
        assert!(
            owned.diagnostics().is_empty(),
            "{text}: {:?}",
            owned.diagnostics()
        );
        assert!(
            owned.deferred().is_empty(),
            "{text}: {:?}",
            owned.deferred()
        );
        owned
    })
}

fn control_transfer(
    sources: &SourceMap,
    parsed: &ParsedFile,
    control: UnitExpressionId,
    spelling: &str,
) -> UnitDropPoint {
    let (expression, _) = parsed
        .ast()
        .expressions()
        .iter()
        .find(|(_, node)| {
            matches!(
                node.payload(),
                Expression::Return { .. } | Expression::Break { .. } | Expression::Continue { .. }
            ) && sources.slice(node.span()) == Ok(spelling)
        })
        .expect("original exit expression");
    UnitDropPoint::ControlTransfer(UnitExpressionId::new(control.source_unit(), expression))
}

fn assert_no_when_branch_drops(owned: &CompilationUnitOwnership, control: UnitExpressionId) {
    assert!(
        owned.drops().iter().all(|drop| !matches!(
            drop.point(),
            UnitDropPoint::BranchEntry { control: actual, .. }
                | UnitDropPoint::BranchExit { control: actual, .. } if actual == control
        )),
        "neither branch publishes cleanup: {:?}",
        owned.drops()
    );
}

#[test]
fn transferred_result_owner_gets_its_own_normal_drop() {
    use lang_frontend::ownership_checking::{CleanupCondition, CleanupOwnerValue};
    let owned = checked(
        "class Node {}\nfun test(own x: Node?): Unit { val result = when (x) { null -> Node(); else -> x } }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let parameter = owned.bindings()[0].symbol();
    let result_drops: Vec<_> = owned
        .drops()
        .iter()
        .filter(|fact| matches!(fact.target(), DropTarget::Named(symbol) if symbol != parameter))
        .collect();
    assert_eq!(
        result_drops.len(),
        2,
        "the constructed and extracted results have distinct owner definitions"
    );
    assert_ne!(result_drops[0].owner(), result_drops[1].owner());
    assert!(
        result_drops
            .iter()
            .all(|fact| matches!(fact.point(), DropPoint::AfterStatement(_)))
    );
    assert_eq!(result_drops[0].point(), result_drops[1].point());
    assert!(result_drops.iter().any(
        |fact| matches!(owned.cleanup_conditions().owner_value(fact.owner().unwrap()),
        Some(CleanupOwnerValue::Parameter { symbol, .. }) if *symbol == parameter)
    ));
    // 本例只有一个 nullable alternative；两个动态出口各有且只有一次析构。
    let choices = result_drops
        .iter()
        .map(|fact| {
            let CleanupCondition::Choice { selector, branches } = owned
                .cleanup_conditions()
                .get(fact.condition().unwrap())
                .unwrap()
            else {
                panic!("each result version must be selected by the nullable test");
            };
            (*selector, branches)
        })
        .collect::<Vec<_>>();
    assert_eq!(choices[0].0, choices[1].0);
    for arm in 0..2 {
        assert_eq!(
            choices
                .iter()
                .filter(|(_, branches)| matches!(
                    owned.cleanup_conditions().get(branches[arm]),
                    Some(CleanupCondition::Never)
                ))
                .count(),
            1,
            "the unselected version must never be destroyed"
        );
        assert_eq!(
            choices
                .iter()
                .filter(|(_, branches)| matches!(
                    owned.cleanup_conditions().get(branches[arm]),
                    Some(CleanupCondition::Always)
                ))
                .count(),
            1,
            "the transferred result owns exactly one cleanup obligation on each path"
        );
    }
}

#[test]
fn named_subject_stays_live_for_future_loop_iterations() {
    for owned in checked_nullable_unit(
        "class Node {}\nfun read(x: Node): Unit {}\nfun test(flag: Boolean, own x: Node?): Unit { while (flag) { val result = when (x) { null -> {}; else -> read(x) } } }",
    ) {
        let [plan] = owned.nullable_whens() else {
            panic!("one nullable when");
        };
        assert_no_when_branch_drops(&owned, plan.descriptor().expression());
        let root = plan.source_place().expect("named subject").root();
        let points: Vec<_> = owned
            .drops()
            .iter()
            .filter(|drop| drop.target() == UnitDropTarget::Named(root))
            .map(|drop| drop.point())
            .collect();
        assert!(
            matches!(points.as_slice(), [UnitDropPoint::LoopExit(loop_id)]
            if loop_id.source_unit() == plan.descriptor().expression().source_unit()),
            "the owner must survive the loop backedge: {points:?}"
        );
    }
}

#[test]
fn earlier_borrow_argument_temporary_is_cleaned_on_later_control_transfer() {
    for (body, spelling) in [
        (
            "fun test(y: Int?): Unit = outer(Node(), when (y) { null -> {}; else -> return })",
            "return",
        ),
        (
            "fun test(y: Int?): Unit { loop { val result = outer(Node(), when (y) { null -> {}; else -> break }) } }",
            "break",
        ),
        (
            "fun test(y: Int?): Unit { loop { val result = outer(Node(), when (y) { null -> {}; else -> continue }) } }",
            "continue",
        ),
    ] {
        let text =
            format!("class Node {{}}\nfun outer(x: Node, own result: Unit): Unit {{}}\n{body}");
        for rooted in [false, true] {
            let (sources, parsed, owned) = nullable_unit::checked(
                &text,
                "nullable-ownership.ko",
                rooted,
                standard_environments(),
            );
            assert!(
                owned.diagnostics().is_empty(),
                "{body}: {:?}",
                owned.diagnostics()
            );
            assert!(
                owned.deferred().is_empty(),
                "{body}: {:?}",
                owned.deferred()
            );
            let [plan] = owned.nullable_whens() else {
                panic!("one nullable when: {body}");
            };
            let transfer =
                control_transfer(&sources, &parsed, plan.descriptor().expression(), spelling);
            let ends: Vec<_> = owned
                .direct_call_loan_ends()
                .iter()
                .filter(|end| end.point() == transfer)
                .collect();
            let [end] = ends.as_slice() else {
                panic!("one actual loan end at {spelling}: {ends:?}");
            };
            let loan = end.loan();
            assert_eq!(loan.kind(), LoanKind::Shared);
            assert_eq!(loan.target(), &UnitLoanTarget::Temporary(loan.argument()));
            let argument_span = parsed
                .ast()
                .expressions()
                .get(loan.argument().expression())
                .unwrap()
                .span();
            assert_eq!(sources.slice(argument_span), Ok("Node()"));
            assert_eq!(end.preceding_drops(), 0);
            let points: Vec<_> = owned
                .drops()
                .iter()
                .filter(|drop| drop.target() == UnitDropTarget::Temporary(loan.argument()))
                .map(|drop| drop.point())
                .collect();
            assert_eq!(points.len(), 2, "{body}: {points:?}");
            assert!(points.contains(&transfer));
            assert!(points.contains(&UnitDropPoint::CallReturn(loan.call())));
        }
    }
}

#[test]
fn earlier_borrowed_root_survives_later_when_until_call_return() {
    for owned in checked_nullable_unit(
        "class Node {}\nfun outer(x: Node, own result: Unit): Unit {}\nfun test(own x: Node, y: Int?): Unit = outer(x, when (y) { null -> {}; else -> {} })",
    ) {
        let [plan] = owned.nullable_whens() else {
            panic!("one nullable when");
        };
        assert_no_when_branch_drops(&owned, plan.descriptor().expression());
        assert!(plan.entries().iter().all(|entry| entry.falls_through()));
        let [end] = owned.direct_call_loan_ends() else {
            panic!("one actual Shared loan end");
        };
        let UnitLoanTarget::Place(place) = end.loan().target() else {
            panic!("earlier argument borrows the named root");
        };
        assert!(place.is_root());
        assert_eq!(end.loan().kind(), LoanKind::Shared);
        assert_eq!(
            end.loan().call().source_unit(),
            plan.descriptor().expression().source_unit()
        );
        assert_eq!(end.point(), UnitDropPoint::CallReturn(end.loan().call()));
        assert_eq!(end.preceding_drops(), 0);
        let points: Vec<_> = owned
            .drops()
            .iter()
            .filter(|drop| drop.target() == UnitDropTarget::Named(place.root()))
            .map(|drop| drop.point())
            .collect();
        assert_eq!(
            points,
            [end.point()],
            "the root survives until the outer loan ends"
        );
    }
}

#[test]
fn element_subject_read_still_checks_an_active_exclusive_loan() {
    for rooted in [false, true] {
        let checked = |text: &str| {
            nullable_unit::checked(
                text,
                "nullable-ownership.ko",
                rooted,
                standard_environments(),
            )
            .2
        };

        let owned = checked(
            "class Node {}\nfun check(inout xs: List<Node?>, own result: Unit): Unit {}\nfun test(own input: List<Node?>): Unit { var xs = input; val result = check(&xs, when (xs[0]) { null -> {}; else -> {} }) }",
        );
        let codes: Vec<_> = owned
            .diagnostics()
            .iter()
            .map(|d| d.code().to_string())
            .collect();
        assert_eq!(
            codes,
            ["L0135"],
            "a read-only subject must not bypass loan conflicts"
        );
        assert!(owned.nullable_whens().is_empty());
    }
}

#[test]
fn temporary_subject_cleanup_follows_return_and_loop_transfer_edges() {
    for (body, transfer) in [
        (
            "fun test(): Int = when (make()) { null -> 0; else -> return 1 }",
            "return 1",
        ),
        (
            "fun test(): Unit { loop { val selected = when (make()) { null -> {}; else -> break } } }",
            "break",
        ),
        (
            "fun test(): Unit { loop { val selected = when (make()) { null -> {}; else -> continue } } }",
            "continue",
        ),
    ] {
        let text = format!("class Node {{}}\nfun make(): Node? = Node()\n{body}");
        for rooted in [false, true] {
            let (sources, parsed, owned) = nullable_unit::checked(
                &text,
                "nullable-ownership.ko",
                rooted,
                standard_environments(),
            );
            assert!(
                owned.diagnostics().is_empty(),
                "{body}: {:?}",
                owned.diagnostics()
            );
            assert!(
                owned.deferred().is_empty(),
                "{body}: {:?}",
                owned.deferred()
            );
            let [plan] = owned.nullable_whens() else {
                panic!("one nullable when: {body}");
            };
            let [normal, exit] = plan.entries() else {
                panic!("both entries analyzed: {body}");
            };
            assert!(normal.falls_through());
            assert_eq!(
                (exit.falls_through(), exit.breaks(), exit.continues()),
                (false, transfer == "break", transfer == "continue")
            );
            let control = plan.descriptor().expression();
            let subject = plan.descriptor().subject();
            let expected_exit = control_transfer(&sources, &parsed, control, transfer);
            let drops: Vec<_> = owned
                .drops()
                .iter()
                .filter(|drop| drop.target() == UnitDropTarget::Temporary(subject))
                .collect();
            assert_eq!(drops.len(), 2, "{body}: {drops:?}");
            let subject_span = parsed
                .ast()
                .expressions()
                .get(subject.expression())
                .unwrap()
                .span();
            assert!(drops.iter().all(|drop| drop.value_origin() == subject_span));
            assert!(
                drops
                    .iter()
                    .any(|drop| drop.point() == UnitDropPoint::AfterExpression(control))
            );
            assert!(drops.iter().any(|drop| drop.point() == expected_exit));
        }
    }
}

#[test]
fn abort_does_not_unwind_the_held_temporary_subject() {
    for owned in checked_nullable_unit(
        "class Node {}\nfun make(): Node? = Node()\nfun test(): Unit = when (make()) { null -> {}; else -> error(\"stop\") }",
    ) {
        let [plan] = owned.nullable_whens() else {
            panic!("one nullable when");
        };
        let [normal, abort] = plan.entries() else {
            panic!("both entries analyzed");
        };
        assert!(normal.falls_through());
        assert!(!abort.falls_through() && !abort.breaks() && !abort.continues());
        let points: Vec<_> = owned
            .drops()
            .iter()
            .filter(|drop| drop.target() == UnitDropTarget::Temporary(plan.descriptor().subject()))
            .map(|drop| drop.point())
            .collect();
        assert_eq!(
            points,
            [UnitDropPoint::AfterExpression(
                plan.descriptor().expression()
            )],
            "only the continuing path cleans the held subject"
        );
    }
}

#[test]
fn nested_control_preserves_valid_outer_subject_proof() {
    for (text, count) in [
        (
            "fun test(x: Boolean?): Boolean = when (x) { null -> false; else -> { val ignored = when (x) { true -> 0; false -> 1 }; return x } }",
            1,
        ),
        (
            "fun test(x: Int?, y: Int?): Int = when (x) { null -> 0; else -> { val ignored = when (y) { null -> 0; else -> y }; return x } }",
            2,
        ),
    ] {
        let owned = checked(text);
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert_eq!(owned.nullable_whens().len(), count);
        assert!(
            owned
                .nullable_whens()
                .iter()
                .all(|plan| plan.extractions().len() == 1
                    && plan.extractions()[0].kind() == NullableWhenExtractionKind::Copy)
        );
    }
}

#[test]
fn unconsumed_temporary_subject_is_cleaned_on_the_normal_non_null_edge() {
    for owned in checked_nullable_unit(
        "class Node {}\nfun make(): Node? = Node()\nfun test(): Unit = when (make()) { null -> {}; else -> {} }",
    ) {
        let [plan] = owned.nullable_whens() else {
            panic!("one nullable when");
        };
        assert_eq!(plan.entries().len(), 2);
        let [null, non_null] = plan.descriptor().entries() else {
            panic!("both nullable domains");
        };
        assert_eq!(null.body_domain(), UnitNullableWhenDomain::Null);
        assert_eq!(non_null.body_domain(), UnitNullableWhenDomain::NonNull);
        assert!(plan.entries().iter().all(|entry| entry.falls_through()));
        let points: Vec<_> = owned
            .drops()
            .iter()
            .filter(|drop| drop.target() == UnitDropTarget::Temporary(plan.descriptor().subject()))
            .map(|drop| drop.point())
            .collect();
        assert_eq!(
            points,
            [UnitDropPoint::AfterExpression(
                plan.descriptor().expression()
            )],
            "both normal paths share one cleanup of the evaluated subject"
        );
    }
}

#[test]
fn non_null_condition_value_delivery_is_recorded_before_selecting_body() {
    let owned = checked(
        "fun identity(own x: Boolean): Boolean = x\nfun test(x: Boolean?): Int = when (x) { null -> 0; identity(x) -> 1; else -> 2 }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let extractions = owned.nullable_whens()[0].extractions();
    assert_eq!(
        extractions.len(),
        1,
        "the later condition uses the original subject's non-null value"
    );
    assert_eq!(extractions[0].kind(), NullableWhenExtractionKind::Copy);
    assert_eq!(extractions[0].entry(), 1);
    assert_eq!(extractions[0].alternative(), Some(0));
}

#[test]
fn aborting_nullable_branch_has_no_normal_outcome() {
    for owned in checked_nullable_unit(
        "fun test(x: Int?): Int = when (x) { null -> 0; else -> error(\"stop\") }",
    ) {
        let [plan] = owned.nullable_whens() else {
            panic!("one nullable when");
        };
        let [null, abort] = plan.entries() else {
            panic!("both source branches checked");
        };
        assert_eq!((null.index(), abort.index()), (0, 1));
        assert!(null.falls_through());
        assert!(!abort.falls_through() && !abort.breaks() && !abort.continues());
        let [null_descriptor, abort_descriptor] = plan.descriptor().entries() else {
            panic!("both typed branches");
        };
        assert_eq!(null.body(), null_descriptor.body());
        assert_eq!(abort.body(), abort_descriptor.body());
    }
}

#[test]
fn branch_summary_retains_partial_consumption_without_claiming_every_path_moves() {
    let owned = checked(
        "class Node {}\nfun take(own x: Node): Unit {}\nfun test(flag: Boolean, own x: Node?): Unit = when (x) { null -> {}; else -> if (flag) { take(x) } else {} }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let plan = &owned.nullable_whens()[0];
    assert_eq!(
        plan.branches()[0].outcome(),
        NullableWhenBranchOutcome::Null
    );
    assert_eq!(
        plan.branches()[1].outcome(),
        NullableWhenBranchOutcome::MayConsume
    );
    assert_eq!(plan.extractions().len(), 1);
    assert_eq!(plan.extractions()[0].entry(), 1);
}

#[test]
fn copyable_inner_copy_is_legal_while_subject_has_a_shared_loan() {
    let owned = checked(
        "fun combine(x: Int?, own y: Int): Int = y\nfun test(x: Int?): Int = combine(x, when (x) { null -> 0; else -> x })",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert_eq!(owned.nullable_whens()[0].extractions().len(), 1);
    assert_eq!(
        owned.nullable_whens()[0].extractions()[0].kind(),
        NullableWhenExtractionKind::Copy
    );
}

#[test]
fn replacement_does_not_inherit_the_original_subject_extraction_proof() {
    let owned = checked(
        "fun test(input: Int?, replacement: Int?): Int? { var x = input; return (when (x) { null -> null; else -> { x = replacement; if (true) { x } else { x } } }) }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert_eq!(owned.nullable_whens().len(), 1);
    assert!(
        owned.nullable_whens()[0].extractions().is_empty(),
        "a new nullable value is not the old subject's inner value"
    );
}

#[test]
fn inner_value_delivery_records_copy_or_consume_but_read_records_neither() {
    for (text, kind) in [
        (
            "fun test(x: Int?): Int = when (x) { null -> 0; else -> x }",
            Some(NullableWhenExtractionKind::Copy),
        ),
        (
            "class Node {}\nfun test(own x: Node?): Node = when (x) { null -> Node(); else -> x }",
            Some(NullableWhenExtractionKind::Consume),
        ),
        (
            "class Node {}\nfun read(x: Node): Unit {}\nfun test(x: Node?): Unit = when (x) { null -> {}; else -> read(x) }",
            None,
        ),
    ] {
        let owned = checked(text);
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        let plans = owned.nullable_whens();
        assert_eq!(plans.len(), 1);
        let extractions = plans[0].extractions();
        assert_eq!(extractions.len(), usize::from(kind.is_some()));
        if let Some(kind) = kind {
            assert_eq!(extractions[0].kind(), kind);
            assert_eq!(extractions[0].entry(), 1);
            assert_eq!(extractions[0].alternative(), None);
            assert_ne!(extractions[0].expression(), plans[0].subject());
        }
    }
}

#[test]
fn nullable_proof_keeps_the_single_subject_identity() {
    for text in [
        "class Node {}\nclass Holder(val item: Node?)\nfun read(x: Node?): Unit {}\nfun test(h: Holder): Unit = when (h.item) { null -> {}; else -> read(h.item) }",
        "class Node {}\nfun read(x: Node?): Unit {}\nfun test(xs: List<Node?>): Unit = when (xs[0]) { null -> {}; else -> read(xs[0]) }",
    ] {
        let owned = checked(text);
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert_eq!(owned.nullable_whens().len(), 1);
        let plan = &owned.nullable_whens()[0];
        assert!(owned.nullable_when(plan.expression()).is_some());
        let view = plan.branches()[1]
            .view()
            .expect("non-null internal subject view");
        assert_eq!(view.subject(), plan.subject());
        assert!(
            view.stable_symbol().is_none(),
            "repeated field/index reads are not stable smart-cast keys"
        );
        assert!(
            view.place().is_some(),
            "proof must retain the existing place identity"
        );
        assert!(
            plan.extractions().is_empty(),
            "borrowing the repeated read is not inner extraction"
        );
    }
}

#[test]
fn invalid_nullable_delivery_publishes_no_partial_plan() {
    for rooted in [false, true] {
        let checked = |text: &str| {
            nullable_unit::checked(
                text,
                "nullable-ownership.ko",
                rooted,
                standard_environments(),
            )
            .2
        };

        let owned = checked(
            "class Node {}\nfun bad(x: Node?): Node = when (x) { null -> Node(); else -> x }",
        );
        assert!(!owned.diagnostics().is_empty());
        assert!(owned.nullable_whens().is_empty());
        assert!(owned.drops().is_empty());
    }
}

#[test]
fn control_result_delivery_cannot_move_a_borrowed_owner() {
    for rooted in [false, true] {
        let checked = |text: &str| {
            nullable_unit::checked(
                text,
                "nullable-ownership.ko",
                rooted,
                standard_environments(),
            )
            .2
        };

        // If and when must apply the same Value contract to their result branches.
        for text in [
            "class Node {}\nfun extract(flag: Boolean, x: Node): Node = if (flag) { x } else { Node() }",
            "class Node {}\nfun extract(flag: Boolean, x: Node): Node = when (flag) { true -> x; false -> Node() }",
        ] {
            let owned = checked(text);
            let codes: Vec<_> = owned
                .diagnostics()
                .iter()
                .map(|d| d.code().to_string())
                .collect();
            assert_eq!(codes, ["L0133"], "{text}");
        }
    }
}

#[test]
fn transferred_control_result_keeps_source_moved_after_join() {
    for rooted in [false, true] {
        let checked = |text: &str| {
            nullable_unit::checked(
                text,
                "nullable-ownership.ko",
                rooted,
                standard_environments(),
            )
            .2
        };

        let owned = checked(
            "class Node {}\nfun test(own x: Node?): Unit { val result = when (x) { null -> Node(); else -> x }; val reused = x }",
        );
        let codes: Vec<_> = owned
            .diagnostics()
            .iter()
            .map(|d| d.code().to_string())
            .collect();
        assert_eq!(
            codes,
            ["L0131"],
            "result delivery must consume the original owner"
        );
        assert!(owned.drops().is_empty());
    }
}

#[test]
fn copyable_inner_preserves_borrow_and_inout_capabilities() {
    // A proof permits copying the inner value without acquiring the caller's owner.
    for (text, capability) in [
        (
            "fun extract(x: Int?): Int = when (x) { null -> 0; else -> x }",
            OwnershipBindingKind::Shared,
        ),
        (
            "fun extract(inout x: Int?): Int = when (x) { null -> 0; else -> x }",
            OwnershipBindingKind::Exclusive,
        ),
        (
            "value class Token(val item: Int)\nfun extract(x: Token?): Token = when (x) { null -> Token(0); else -> x }",
            OwnershipBindingKind::Shared,
        ),
        (
            "enum class Flag { On, Off }\nfun extract(x: Flag?): Flag = when (x) { null -> Flag.Off; else -> x }",
            OwnershipBindingKind::Shared,
        ),
    ] {
        for owned in checked_nullable_unit(text) {
            let [plan] = owned.nullable_whens() else {
                panic!("original nullable subject plan: {text}");
            };
            let root = plan.source_place().expect("named subject").root();
            assert_eq!(owned.binding_kind(root), Some(capability), "{text}");
            assert!(
                !owned
                    .drops()
                    .iter()
                    .any(|drop| drop.target() == UnitDropTarget::Named(root))
            );
            assert_eq!(plan.entries().len(), 2);
            assert!(plan.entries().iter().all(|entry| entry.falls_through()));
        }
    }
}

#[test]
fn owned_move_only_inner_can_transfer_the_whole_root() {
    for text in [
        "class Node {}\nvalue class Wrapped(val node: Node)\nfun extract(own x: Wrapped?): Wrapped = when (x) { null -> Wrapped(Node()); else -> x }",
        "class Node {}\nenum class Event { Empty, Full(node: Node) }\nfun extract(own x: Event?): Event = when (x) { null -> Event.Empty; else -> x }",
        "fun extract(own x: String?): String = when (x) { null -> \"\"; else -> x }",
        "class Node {}\nfun extract(own x: Node?): Node = when (x) { null -> Node(); else -> x }",
        "value class Token(val item: Int)\nfun extract(own x: Box<Token>?, own fallback: Box<Token>): Box<Token> = when (x) { null -> fallback; else -> x }",
        "fun extract(own x: Rc<Int>?, own fallback: Rc<Int>): Rc<Int> = when (x) { null -> fallback; else -> x }",
    ] {
        let owned = checked(text);
        assert!(
            owned.diagnostics().is_empty(),
            "{text}: {:?}",
            owned.diagnostics()
        );
        assert!(
            owned.rc_effects().is_empty(),
            "whole-root extraction must not introduce retain/share effects"
        );
        assert_eq!(
            owned.nullable_whens()[0].extractions()[0].kind(),
            NullableWhenExtractionKind::Consume
        );
    }
}

#[test]
fn proof_does_not_grant_move_permission_to_borrow_or_inout() {
    for rooted in [false, true] {
        let checked = |text: &str| {
            nullable_unit::checked(
                text,
                "nullable-ownership.ko",
                rooted,
                standard_environments(),
            )
            .2
        };

        for mode in ["", "inout "] {
            let text = format!(
                "class Node {{}}\nfun extract({mode}x: Node?): Node = when (x) {{ null -> Node(); else -> x }}"
            );
            let owned = checked(&text);
            let codes: Vec<_> = owned
                .diagnostics()
                .iter()
                .map(|d| d.code().to_string())
                .collect();
            assert_eq!(codes, ["L0133"], "{text}");
            assert!(
                owned.drops().is_empty(),
                "invalid ownership must not publish drop plans"
            );
        }
    }
}

#[test]
fn repeated_field_and_element_reads_cannot_reuse_subject_proof_for_move() {
    for rooted in [false, true] {
        let checked = |text: &str| {
            nullable_unit::checked(
                text,
                "nullable-ownership.ko",
                rooted,
                standard_environments(),
            )
            .2
        };

        // Return nullable to avoid pretending the repeated read has the internal subject's proof.
        for (text, code) in [
            (
                "class Node {}\nclass Holder(val item: Node?)\nfun extract(own h: Holder): Node? = when (h.item) { null -> null; else -> h.item }",
                "L0132",
            ),
            (
                "class Node {}\nfun extract(own xs: List<Node?>): Node? = when (xs[0]) { null -> null; else -> xs[0] }",
                "L0136",
            ),
        ] {
            let owned = checked(text);
            let codes: Vec<_> = owned
                .diagnostics()
                .iter()
                .map(|d| d.code().to_string())
                .collect();
            assert_eq!(codes, [code], "{text}");
        }
    }
}

#[test]
fn nullable_extraction_respects_active_loan_and_previous_move() {
    for rooted in [false, true] {
        let checked = |text: &str| {
            nullable_unit::checked(
                text,
                "nullable-ownership.ko",
                rooted,
                standard_environments(),
            )
            .2
        };

        for (text, code) in [
            (
                "class Node {}\nfun take(own x: Node): Unit {}\nfun outer(x: Node?, own result: Unit): Unit {}\nfun test(own x: Node?): Unit = outer(x, when (x) { null -> {}; else -> take(x) })",
                "L0135",
            ),
            (
                "class Node {}\nfun take(own x: Node): Unit {}\nfun test(own x: Node?): Unit = when (x) { null -> {}; else -> { val first = take(x); val second = take(x) } }",
                "L0131",
            ),
        ] {
            let owned = checked(text);
            let codes: Vec<_> = owned
                .diagnostics()
                .iter()
                .map(|d| d.code().to_string())
                .collect();
            assert_eq!(codes, [code], "{text}");
            assert!(owned.drops().is_empty());
        }
    }
}

#[test]
fn read_only_when_does_not_consume_named_subject_needed_after_join() {
    for owned in checked_nullable_unit(
        "class Node {}\nfun read(x: Node?): Unit {}\nfun test(own x: Node?): Unit { val result = when (x) { null -> {}; else -> read(x) }; val later = read(x) }",
    ) {
        let [plan] = owned.nullable_whens() else {
            panic!("one nullable when");
        };
        let root = plan.source_place().expect("named subject").root();
        let mut reads = owned.loans().iter().filter(
            |loan| matches!(loan.target(), UnitLoanTarget::Place(place) if place.root() == root),
        );
        let inside = reads.next().expect("branch borrows the original subject");
        let later = reads
            .next()
            .expect("post-join read borrows the same subject");
        assert!(reads.next().is_none());
        assert!(inside.begin_span().start() < later.begin_span().start());
        assert_ne!(inside.call(), later.call());
        assert_eq!([inside.kind(), later.kind()], [LoanKind::Shared; 2]);
        let root_drops: Vec<_> = owned
            .drops()
            .iter()
            .filter(|drop| drop.target() == UnitDropTarget::Named(root))
            .collect();
        assert_eq!(root_drops.len(), 1);
        assert_eq!(
            root_drops[0].point(),
            UnitDropPoint::CallReturn(later.call())
        );
    }
}

#[test]
fn earlier_comma_match_keeps_drop_obligation_skipped_by_later_move() {
    // Only probe owns the later edge. The null edge must still destroy the unused resource.
    for owned in checked_nullable_unit(
        "class Resource {}\nfun probe(own resource: Resource): Boolean = true\nfun test(flag: Boolean?, own resource: Resource): Int = when (flag) { null, probe(resource) -> 0; else -> 0 }",
    ) {
        let drops: Vec<_> = owned
            .drops()
            .iter()
            .filter(|fact| matches!(fact.target(), UnitDropTarget::Named(_)))
            .collect();
        assert_eq!(drops.len(), 2, "each callable owns one drop: {drops:?}");
        assert_ne!(drops[0].target(), drops[1].target());
        assert!(
            drops.iter().any(|drop| matches!(
                drop.point(),
                UnitDropPoint::WhenAlternativeMatch {
                    entry: 0,
                    alternative: 0,
                    ..
                }
            )),
            "early match retains the cleanup skipped by the later move: {drops:?}"
        );
    }
}
