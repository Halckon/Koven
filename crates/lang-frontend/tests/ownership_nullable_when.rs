//! SPEC-0203 nullable branch views preserve the original owner's capabilities.

use lang_frontend::{
    name_resolution::resolve_names,
    ownership_checking::{
        DropPoint, DropTarget, LoanEndPoint, NullableWhenBranchOutcome, NullableWhenExtractionKind,
        OwnershipCheckedFile, check_ownership,
    },
    source::SourceMap,
    type_checking::{check_types, standard_environments},
};

#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

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

#[test]
fn transferred_result_owner_gets_its_own_normal_drop() {
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
        1,
        "the transferred result owns exactly one cleanup obligation"
    );
    assert!(matches!(
        result_drops[0].point(),
        DropPoint::AfterStatement(_)
    ));
}

#[test]
fn named_subject_stays_live_for_future_loop_iterations() {
    let owned = checked(
        "class Node {}\nfun read(x: Node): Unit {}\nfun test(flag: Boolean, own x: Node?): Unit { while (flag) { val result = when (x) { null -> {}; else -> read(x) } } }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let plan = &owned.nullable_whens()[0];
    assert!(
        plan.branches()
            .iter()
            .all(|branch| branch.drops().is_empty()),
        "normal branch exits must preserve the owner for the loop backedge"
    );
    assert!(
        owned
            .drops()
            .iter()
            .any(|fact| matches!(fact.point(), DropPoint::LoopExit(_)))
    );
}

#[test]
fn earlier_borrow_argument_temporary_is_cleaned_on_later_control_transfer() {
    for body in [
        "fun test(y: Int?): Unit = outer(Node(), when (y) { null -> {}; else -> return })",
        "fun test(y: Int?): Unit { loop { val result = outer(Node(), when (y) { null -> {}; else -> break }) } }",
        "fun test(y: Int?): Unit { loop { val result = outer(Node(), when (y) { null -> {}; else -> continue }) } }",
    ] {
        let owned = checked(&format!(
            "class Node {{}}\nfun outer(x: Node, own result: Unit): Unit {{}}\n{body}"
        ));
        assert!(
            owned.diagnostics().is_empty(),
            "{body}: {:?}",
            owned.diagnostics()
        );
        let branch = &owned.nullable_whens()[0].branches()[1];
        assert!(
            branch
                .drops()
                .iter()
                .any(|fact| matches!(fact.target(), DropTarget::Temporary(_))
                    && matches!(fact.point(), DropPoint::ControlTransfer(_))),
            "{body}: the already evaluated borrow temporary must be cleaned before leaving the call"
        );
        for drop in branch
            .drops()
            .iter()
            .filter(|fact| matches!(fact.target(), DropTarget::Temporary(_)))
        {
            if let DropPoint::ControlTransfer(transfer) = drop.point() {
                assert!(
                    owned
                        .loan_ends()
                        .iter()
                        .any(|end| end.point() == LoanEndPoint::ControlTransfer(transfer)),
                    "temporary cleanup must have an explicit loan end on the same transfer edge"
                );
            }
        }
    }
}

#[test]
fn earlier_borrowed_root_survives_later_when_until_call_return() {
    let owned = checked(
        "class Node {}\nfun outer(x: Node, own result: Unit): Unit {}\nfun test(own x: Node, y: Int?): Unit = outer(x, when (y) { null -> {}; else -> {} })",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let plan = &owned.nullable_whens()[0];
    assert!(
        plan.branches()
            .iter()
            .all(|branch| branch.drops().is_empty()),
        "the first argument's active loan keeps its root live across the second argument"
    );
    assert!(
        owned
            .drops()
            .iter()
            .any(|fact| matches!(fact.target(), DropTarget::Named(_))
                && matches!(fact.point(), DropPoint::CallReturn(_)))
    );
    assert!(
        owned
            .loan_ends()
            .iter()
            .any(|end| matches!(end.point(), LoanEndPoint::CallReturn(_)))
    );
}

#[test]
fn element_subject_read_still_checks_an_active_exclusive_loan() {
    let owned = checked(
        "class Node {}\nfun check(inout xs: List<Node?>, own result: Unit): Unit {}\nfun test(own input: List<Node?>): Unit { var xs = input val result = check(&xs, when (xs[0]) { null -> {}; else -> {} }) }",
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

#[test]
fn temporary_subject_cleanup_follows_return_and_loop_transfer_edges() {
    for body in [
        "fun test(): Int = when (make()) { null -> 0; else -> return 1 }",
        "fun test(): Unit { loop { val selected = when (make()) { null -> {}; else -> break } } }",
        "fun test(): Unit { loop { val selected = when (make()) { null -> {}; else -> continue } } }",
    ] {
        let owned = checked(&format!(
            "class Node {{}}\nfun make(): Node? = Node()\n{body}"
        ));
        assert!(
            owned.diagnostics().is_empty(),
            "{body}: {:?}",
            owned.diagnostics()
        );
        let plan = &owned.nullable_whens()[0];
        let cleanup: Vec<_> = plan.branches()[1]
            .drops()
            .iter()
            .filter(|fact| fact.target() == DropTarget::Temporary(plan.subject()))
            .collect();
        assert_eq!(
            cleanup.len(),
            1,
            "{body}: each transfer must clean the held subject once"
        );
        assert!(matches!(cleanup[0].point(), DropPoint::ControlTransfer(_)));
        assert_eq!(
            plan.branches()[1].outcome(),
            NullableWhenBranchOutcome::Diverging
        );
    }
}

#[test]
fn abort_does_not_unwind_the_held_temporary_subject() {
    let owned = checked(
        "class Node {}\nfun make(): Node? = Node()\nfun test(): Unit = when (make()) { null -> {}; else -> error(\"stop\") }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let plan = &owned.nullable_whens()[0];
    assert_eq!(
        plan.branches()[1].outcome(),
        NullableWhenBranchOutcome::Diverging
    );
    assert!(
        !plan.branches()[1]
            .drops()
            .iter()
            .any(|fact| fact.target() == DropTarget::Temporary(plan.subject())),
        "abort has no owner cleanup edge"
    );
}

#[test]
fn nested_control_preserves_valid_outer_subject_proof() {
    for (text, count) in [
        (
            "fun test(x: Boolean?): Boolean = when (x) { null -> false; else -> { val ignored = when (x) { true -> 0; false -> 1 } return x } }",
            1,
        ),
        (
            "fun test(x: Int?, y: Int?): Int = when (x) { null -> 0; else -> { val ignored = when (y) { null -> 0; else -> y } return x } }",
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
    let owned = checked(
        "class Node {}\nfun make(): Node? = Node()\nfun test(): Unit = when (make()) { null -> {}; else -> {} }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let plan = &owned.nullable_whens()[0];
    assert!(
        plan.branches()[1]
            .drops()
            .iter()
            .any(|fact| fact.target() == DropTarget::Temporary(plan.subject())),
        "the internal temporary owner must survive the null test and be cleaned on this edge"
    );
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
    let owned = checked("fun test(x: Int?): Int = when (x) { null -> 0; else -> error(\"stop\") }");
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert_eq!(
        owned.nullable_whens()[0].branches()[1].outcome(),
        NullableWhenBranchOutcome::Diverging
    );
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
        "fun test(input: Int?, replacement: Int?): Int? { var x = input return (when (x) { null -> null; else -> { x = replacement if (true) { x } else { x } } }) }",
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
    let owned =
        checked("class Node {}\nfun bad(x: Node?): Node = when (x) { null -> Node(); else -> x }");
    assert!(!owned.diagnostics().is_empty());
    assert!(owned.nullable_whens().is_empty());
    assert!(owned.drops().is_empty());
}

#[test]
fn control_result_delivery_cannot_move_a_borrowed_owner() {
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

#[test]
fn transferred_control_result_keeps_source_moved_after_join() {
    let owned = checked(
        "class Node {}\nfun test(own x: Node?): Unit { val result = when (x) { null -> Node(); else -> x } val reused = x }",
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

#[test]
fn copyable_inner_preserves_borrow_and_inout_capabilities() {
    // A proof permits copying the inner value without acquiring the caller's owner.
    for text in [
        "fun extract(x: Int?): Int = when (x) { null -> 0; else -> x }",
        "fun extract(inout x: Int?): Int = when (x) { null -> 0; else -> x }",
        "value class Token(val item: Int)\nfun extract(x: Token?): Token = when (x) { null -> Token(0); else -> x }",
        "enum class Flag { On, Off }\nfun extract(x: Flag?): Flag = when (x) { null -> Flag.Off; else -> x }",
    ] {
        let owned = checked(text);
        assert!(
            owned.diagnostics().is_empty(),
            "{text}: {:?}",
            owned.diagnostics()
        );
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

#[test]
fn repeated_field_and_element_reads_cannot_reuse_subject_proof_for_move() {
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

#[test]
fn nullable_extraction_respects_active_loan_and_previous_move() {
    for (text, code) in [
        (
            "class Node {}\nfun take(own x: Node): Unit {}\nfun outer(x: Node?, own result: Unit): Unit {}\nfun test(own x: Node?): Unit = outer(x, when (x) { null -> {}; else -> take(x) })",
            "L0135",
        ),
        (
            "class Node {}\nfun take(own x: Node): Unit {}\nfun test(own x: Node?): Unit = when (x) { null -> {}; else -> { val first = take(x) val second = take(x) } }",
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

#[test]
fn read_only_when_does_not_consume_named_subject_needed_after_join() {
    let owned = checked(
        "class Node {}\nfun read(x: Node?): Unit {}\nfun test(own x: Node?): Unit { val result = when (x) { null -> {}; else -> read(x) } val later = read(x) }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
}

#[test]
fn earlier_comma_match_keeps_drop_obligation_skipped_by_later_move() {
    // Only probe owns the later edge. The null edge must still destroy the unused resource.
    let owned = checked(
        "class Resource {}\nfun probe(own resource: Resource): Boolean = true\nfun test(flag: Boolean?, own resource: Resource): Int = when (flag) { null, probe(resource) -> 0; else -> 0 }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    // One named drop belongs to probe's parameter; another must cover test's null path.
    assert_eq!(
        owned
            .drops()
            .iter()
            .filter(|fact| matches!(fact.target(), DropTarget::Named(_)))
            .count(),
        2,
        "comma match must retain the early edge's owner obligation: {:?}",
        owned.drops()
    );
}
